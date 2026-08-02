//! Running child processes and reporting what they did.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{Error, Result};

/// Runs a command in the workspace root with inherited output, failing on a non-zero status.
pub(crate) fn run(root: &Path, program: &str, args: &[&str], env: &[(&str, &str)]) -> Result<()> {
    let mut command = Command::new(program);
    command.current_dir(root).args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    let status = command
        .status()
        .map_err(|source| Error::io(program, source))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::failed(format!(
            "`{program} {}` failed with {status}",
            args.join(" ")
        )))
    }
}

/// Runs a command and captures its standard output, failing on a non-zero status.
pub(crate) fn capture(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .current_dir(root)
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|source| Error::io(program, source))?;
    if !output.status.success() {
        return Err(Error::failed(format!(
            "`{program} {}` failed with {}",
            args.join(" "),
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs a command and captures its standard output whether or not it succeeded, with whether it
/// did.
///
/// For the gates whose subject states its own verdict. A run that ends non-zero because it found
/// something has still said what it found, and throwing that away leaves the gate with an exit code
/// where it needs a list; a run that ends non-zero because it never started has said nothing, and
/// the caller is told which it was.
pub(crate) fn capture_outcome(root: &Path, program: &str, args: &[&str]) -> Result<(String, bool)> {
    let output = Command::new(program)
        .current_dir(root)
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|source| Error::io(program, source))?;
    Ok((
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.success(),
    ))
}

/// The cargo binary this xtask was launched by, so a pinned toolchain stays pinned.
pub(crate) fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}
