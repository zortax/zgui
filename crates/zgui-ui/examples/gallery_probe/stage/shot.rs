//! Capturing the window, through the compositor that is showing it.
//!
//! The picture comes from `grim`, which reads the composited output — so what lands in the file is
//! what a person sitting at the machine would see, including the fact that the window is really
//! mapped and really on top. A readback of the render target would prove that this renderer drew
//! those bytes, which is a different and weaker claim about the same frame; this is the one that
//! answers "is it on the screen".

use std::io;
use std::path::PathBuf;
use std::process::Command;

/// The environment variable naming the directory captures go to.
pub(crate) const DIRECTORY: &str = "ZGUI_PROBE_SHOTS";

/// The environment variable naming the window, which is how its geometry is found.
pub(crate) const APP_ID: &str = "ZGUI_PROBE_APPID";

/// Where captures go.
fn directory() -> PathBuf {
    PathBuf::from(std::env::var(DIRECTORY).unwrap_or_else(|_| "target/probe-shots".to_owned()))
}

/// The window's place on the desktop, as `x,y WxH`.
///
/// It is asked for on every capture rather than once, because the window can be moved, and a
/// geometry remembered from the start would silently crop the wrong rectangle out of the screen —
/// which is exactly the kind of picture that looks like a rendering fault and is not one.
fn geometry() -> io::Result<String> {
    let id = std::env::var(APP_ID).unwrap_or_else(|_| "zgui-gal".to_owned());
    let clients = Command::new("hyprctl").args(["-j", "clients"]).output()?;
    let text = String::from_utf8_lossy(&clients.stdout);
    let filter =
        format!(r#".[] | select(.class=="{id}") | "\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])""#);
    let mut child = Command::new("jq")
        .arg("-r")
        .arg(&filter)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    {
        use std::io::Write as _;
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| io::Error::other("jq took no input"))?;
        stdin.write_all(text.as_bytes())?;
    }
    let reply = child.wait_with_output()?;
    let geometry = String::from_utf8_lossy(&reply.stdout).trim().to_owned();
    if geometry.is_empty() {
        return Err(io::Error::other(format!(
            "no window of class {id} is mapped"
        )));
    }
    Ok(geometry)
}

/// Captures the window into `name`.
///
/// # Errors
///
/// Returns what stopped it: a window the compositor does not have mapped, or a capture tool that
/// failed. A capture that cannot be taken is reported rather than skipped, because a missing
/// picture and a picture of the wrong thing are told apart only by saying so.
pub(crate) fn capture(name: &str) -> io::Result<PathBuf> {
    let directory = directory();
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{name}.png"));
    let geometry = geometry()?;
    let status = Command::new("grim")
        .arg("-g")
        .arg(&geometry)
        .arg(&path)
        .status()?;
    if status.success() {
        Ok(path)
    } else {
        Err(io::Error::other(format!("grim exited with {status}")))
    }
}
