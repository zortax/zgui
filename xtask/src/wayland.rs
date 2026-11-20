//! Running the Wayland backend's compositor programs on a nested headless sway.
//!
//! Those programs lock the session, press keys, move the pointer and take the focus, so each one
//! skips unless `ZGUI_NESTED_COMPOSITOR` names the display it connects to. This starts a headless
//! sway, reads its display and its IPC socket, runs the backend's tests against it with the opt-in
//! set, and stops it again. The desktop the run starts from never sees a surface.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::process;

/// The variable the compositor programs read, set to the nested display.
const NESTED: &str = "ZGUI_NESTED_COMPOSITOR";

/// The member whose test targets need a compositor.
const MEMBER: &str = "zgui-platform-wayland";

/// How long sway may take to open its display.
const START_PATIENCE: Duration = Duration::from_secs(10);

/// The variables that point a program at the desktop the run starts from.
///
/// They are removed from the environment of sway and of the tests, so nothing reaches that
/// desktop by mistake.
const SESSION_VARIABLES: &[&str] = &[
    "WAYLAND_DISPLAY",
    "WAYLAND_SOCKET",
    "DISPLAY",
    "SWAYSOCK",
    "HYPRLAND_INSTANCE_SIGNATURE",
    NESTED,
];

/// A running nested sway, stopped when it is dropped.
struct Nested {
    /// The sway process.
    child: Child,
    /// The name of its Wayland display.
    display: String,
    /// The path of its IPC socket.
    ipc: String,
}

impl Drop for Nested {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts a nested headless sway and runs the backend's tests on it.
///
/// `extra` goes to `cargo test` after the package, for example `--test session_lock`.
pub(crate) fn run(root: &Path, extra: &[String]) -> Result<()> {
    let directory = root.join("target").join("nested-sway");
    std::fs::create_dir_all(&directory).map_err(|source| Error::io(&directory, source))?;
    let nested = start(&directory)?;
    println!(
        "nested sway: WAYLAND_DISPLAY={} SWAYSOCK={}",
        nested.display, nested.ipc
    );

    let mut command = Command::new(process::cargo());
    command.current_dir(root).args(["test", "-p", MEMBER]);
    command.args(extra);
    for variable in SESSION_VARIABLES {
        command.env_remove(variable);
    }
    command
        .env("WAYLAND_DISPLAY", &nested.display)
        .env("SWAYSOCK", &nested.ipc)
        .env(NESTED, &nested.display);
    let status = command
        .status()
        .map_err(|source| Error::io("cargo", source))?;
    drop(nested);
    if status.success() {
        Ok(())
    } else {
        Err(Error::failed(format!(
            "the tests of {MEMBER} failed on the nested sway with {status}"
        )))
    }
}

/// Starts sway on the headless backend and waits until it reports its display.
fn start(directory: &Path) -> Result<Nested> {
    let report = directory.join("display");
    let _ = std::fs::remove_file(&report);
    let config = directory.join("config");
    crate::error::write(&config, &config_text(&report))?;
    let log = directory.join("sway.log");
    let log_file = std::fs::File::create(&log).map_err(|source| Error::io(&log, source))?;
    let log_error = log_file
        .try_clone()
        .map_err(|source| Error::io(&log, source))?;

    let mut command = Command::new("sway");
    for variable in SESSION_VARIABLES {
        command.env_remove(variable);
    }
    command
        .arg("--config")
        .arg(&config)
        .env("WLR_BACKENDS", "headless")
        .env("WLR_LIBINPUT_NO_DEVICES", "1")
        .stdout(log_file)
        .stderr(log_error);
    let child = command
        .spawn()
        .map_err(|source| Error::io("sway", source))?;
    let mut nested = Nested {
        child,
        display: String::new(),
        ipc: String::new(),
    };

    let started = Instant::now();
    loop {
        if let Some((display, ipc)) = read_report(&report) {
            nested.display = display;
            nested.ipc = ipc;
            return Ok(nested);
        }
        if let Ok(Some(status)) = nested.child.try_wait() {
            return Err(Error::failed(format!(
                "sway ended with {status} before it opened a display; see {}",
                log.display()
            )));
        }
        if started.elapsed() > START_PATIENCE {
            return Err(Error::failed(format!(
                "sway opened no display within {START_PATIENCE:?}; see {}",
                log.display()
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The sway configuration: no wallpaper, and one command that writes the display and the socket.
///
/// Sway runs `exec` through the shell with its own environment, so the shell expands both
/// variables to the values of this sway. The report appears complete or not at all.
fn config_text(report: &Path) -> String {
    let partial = PathBuf::from(format!("{}.partial", report.display()));
    format!(
        "swaybg_command -\nexec echo $WAYLAND_DISPLAY $SWAYSOCK > {partial} && mv {partial} {report}\n",
        partial = partial.display(),
        report = report.display(),
    )
}

/// The display and the socket that sway wrote, once the report exists.
fn read_report(report: &Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(report).ok()?;
    parse_report(&text)
}

/// Splits a report into the display and the socket.
fn parse_report(text: &str) -> Option<(String, String)> {
    let mut words = text.split_whitespace();
    let display = words.next()?;
    let ipc = words.next()?;
    Some((display.to_owned(), ipc.to_owned()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{config_text, parse_report};

    #[test]
    fn a_complete_report_names_the_display_and_the_socket() {
        assert_eq!(
            parse_report("wayland-3 /run/user/1000/sway-ipc.1000.42.sock\n"),
            Some((
                "wayland-3".to_owned(),
                "/run/user/1000/sway-ipc.1000.42.sock".to_owned()
            ))
        );
    }

    #[test]
    fn a_report_without_both_values_is_incomplete() {
        assert_eq!(parse_report(""), None);
        assert_eq!(parse_report("wayland-3\n"), None);
    }

    #[test]
    fn the_config_moves_a_complete_report_into_place() {
        let text = config_text(Path::new("/x/display"));
        assert!(text.contains("> /x/display.partial && mv /x/display.partial /x/display"));
        assert!(text.starts_with("swaybg_command -\n"));
    }
}
