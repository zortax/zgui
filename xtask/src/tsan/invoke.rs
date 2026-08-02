//! Building and running one test target with the thread sanitiser linked in.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{Error, Result};
use crate::process;
use crate::tsan::subject::{CONTROL_VARIABLE, Subject};

/// The compiler flag that links the sanitiser runtime and instruments every memory access.
const SANITIZER: &str = "-Zsanitizer=thread";

/// The suppression file, relative to the workspace root.
pub(crate) const SUPPRESSIONS: &str = "xtask/tsan-suppressions.txt";

/// Where the sanitised artefacts live.
///
/// A directory of its own, because every object in it is instrumented and mixing them with the
/// ordinary build would mean one of the two is silently rebuilt on every switch.
const TARGET_DIR: &str = "target/tsan";

/// How much history the sanitiser keeps per thread.
///
/// The default loses the older of the two accesses in a long-running traversal and reports a stack
/// with no source in it, which is unreadable rather than absent.
const HISTORY_SIZE: u32 = 7;

/// Everything one sanitised run needs that is the same for every target.
#[derive(Debug, Clone)]
pub(crate) struct Session {
    /// The workspace root.
    root: PathBuf,
    /// The cargo binary, so a pinned toolchain stays pinned.
    cargo: String,
    /// The host target triple, which `-Zbuild-std` requires be named explicitly.
    host: String,
    /// The sanitiser's own options string.
    options: String,
}

impl Session {
    /// Prepares a session, failing when the machine cannot host the sanitiser.
    pub(crate) fn open(root: &Path) -> Result<Self> {
        let cargo = process::cargo();
        let host = host_triple(root)?;
        if !supported(&host) {
            return Err(Error::failed(format!(
                "the thread sanitiser is not supported on `{host}`; it needs an x86_64 or aarch64 \
                 Linux or macOS host"
            )));
        }
        let suppressions = root.join(SUPPRESSIONS);
        if !suppressions.is_file() {
            return Err(Error::failed(format!(
                "no suppression file at {}: without it the engine's own rule tree reports a race \
                 the sanitiser cannot model, and the run would be failed by something that is not \
                 a defect",
                suppressions.display()
            )));
        }
        Ok(Self {
            root: root.to_path_buf(),
            cargo,
            host,
            // `exitcode=0` hands the verdict to the caller: the sanitiser's own exit status cannot
            // tell a reported race from a failed assertion, and the control run needs those to mean
            // opposite things. `halt_on_error=0` lets a run finish so that every report in it is
            // seen rather than only the first.
            options: format!(
                "suppressions={}:history_size={HISTORY_SIZE}:halt_on_error=0:exitcode=0:\
                 print_suppressions=1",
                suppressions.display()
            ),
        })
    }

    /// The environment one run is given.
    fn environment(&self, subject: &Subject) -> Vec<(String, String)> {
        let mut environment = vec![
            ("RUSTFLAGS".to_owned(), SANITIZER.to_owned()),
            (
                "CARGO_TARGET_DIR".to_owned(),
                self.root.join(TARGET_DIR).display().to_string(),
            ),
            ("TSAN_OPTIONS".to_owned(), self.options.clone()),
        ];
        if subject.arms_control() {
            environment.push((CONTROL_VARIABLE.to_owned(), "1".to_owned()));
        }
        environment
    }

    /// The cargo arguments that select one target.
    fn arguments(&self, subject: &Subject) -> Vec<String> {
        [
            "test",
            "-p",
            subject.package,
            "--test",
            subject.test,
            "--release",
            "--target",
            &self.host,
            "-Zbuild-std",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    /// Compiles the target, streaming the build so a long one is visibly progressing.
    pub(crate) fn build(&self, subject: &Subject) -> Result<()> {
        let mut arguments = self.arguments(subject);
        arguments.push("--no-run".to_owned());
        let borrowed: Vec<&str> = arguments.iter().map(String::as_str).collect();
        let environment = self.environment(subject);
        let pairs: Vec<(&str, &str)> = environment
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        process::run(&self.root, &self.cargo, &borrowed, &pairs)
    }

    /// Runs the compiled target and returns everything it printed on both streams.
    ///
    /// The output is the verdict, so it is captured rather than inherited, and echoed afterwards so
    /// that a failure is readable in the log it was produced in.
    pub(crate) fn run(&self, subject: &Subject) -> Result<Run> {
        let mut arguments = self.arguments(subject);
        arguments.extend(
            ["--", "--test-threads=1", "--nocapture"]
                .into_iter()
                .map(str::to_owned),
        );
        let mut command = Command::new(&self.cargo);
        command
            .current_dir(&self.root)
            .args(&arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in self.environment(subject) {
            command.env(key, value);
        }
        let output = command
            .output()
            .map_err(|source| Error::io(&self.cargo, source))?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        print!("{text}");
        Ok(Run {
            output: text,
            finished: output.status.success(),
        })
    }
}

/// What one sanitised invocation produced.
#[derive(Debug, Clone)]
pub(crate) struct Run {
    /// Everything the invocation printed, on both streams.
    pub(crate) output: String,
    /// Whether the target ran to completion and its tests passed.
    ///
    /// The sanitiser is configured to leave the exit status alone, so a report never lowers it.
    /// What does lower it is a failed assertion, a panic, or the runtime dying before it could
    /// watch anything — and each of those makes the run's silence about data races meaningless,
    /// because the code that would have raced never finished running.
    pub(crate) finished: bool,
}

/// Whether the sanitiser runtime exists for `triple`.
fn supported(triple: &str) -> bool {
    const HOSTS: [&str; 4] = [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ];
    HOSTS.contains(&triple)
}

/// The triple the toolchain compiles for by default.
fn host_triple(root: &Path) -> Result<String> {
    let text = process::capture(root, "rustc", &["-vV"])?;
    text.lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .ok_or_else(|| Error::failed("`rustc -vV` printed no host line".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::supported;

    #[test]
    fn only_the_hosts_with_a_sanitiser_runtime_are_accepted() {
        assert!(supported("x86_64-unknown-linux-gnu"));
        assert!(supported("aarch64-apple-darwin"));
        assert!(
            !supported("x86_64-pc-windows-msvc"),
            "a host with no runtime has to be refused rather than run and believed"
        );
        assert!(!supported("wasm32-unknown-unknown"));
    }
}
