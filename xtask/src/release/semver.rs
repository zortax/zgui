//! The compatibility gate.
//!
//! A published crate's public surface is a promise, and the only way to keep it is to have a
//! machine compare the surface against the one that was published last. `cargo-semver-checks` does
//! that comparison; this decides what to compare against and turns its answer into an exit code.
//!
//! The baseline is the newest release tag. Before the first release there is nothing a consumer
//! could have depended on, so the comparison is reported as having no baseline rather than being
//! quietly passed: a gate that says "ok" when it did not run is worse than no gate.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};
use crate::process;

/// The tag prefix a release carries.
const TAG_PREFIX: &str = "v";

/// Runs the compatibility gate.
pub(crate) fn run(root: &Path) -> Result<()> {
    if !installed() {
        return Err(Error::failed(
            "cargo-semver-checks is not installed; `cargo install cargo-semver-checks --locked`"
                .to_owned(),
        ));
    }

    match baseline(root) {
        None => {
            println!(
                "semver     no baseline: no `{TAG_PREFIX}*` tag exists, so nothing has been \
                 published for a consumer to depend on"
            );
            Ok(())
        }
        Some(tag) => {
            println!("semver     comparing the workspace against `{tag}`");
            process::run(
                root,
                &process::cargo(),
                &[
                    "semver-checks",
                    "check-release",
                    // Named explicitly, because the checker resolves a package by scanning the
                    // directory tree rather than by asking the workspace: the ledger's own
                    // planted-violation fixtures carry manifests with real crate names, and
                    // without this the scan finds several packages called `zgui-geom`.
                    "--manifest-path",
                    "Cargo.toml",
                    "--workspace",
                    "--baseline-rev",
                    &tag,
                ],
                &[],
            )
        }
    }
}

/// Whether the checker is available on this machine.
fn installed() -> bool {
    Command::new("cargo")
        .args(["semver-checks", "--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// The newest release tag, or `None` before the first release.
fn baseline(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "tag",
            "--list",
            &format!("{TAG_PREFIX}*"),
            "--sort=-v:refname",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .next()
        .map(str::to_owned)
}
