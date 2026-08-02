//! The lint gate, and why it is run twice.
//!
//! Clippy sees the code as the profile it is invoked in configures it, and the two profiles this
//! workspace ships do not configure it the same way: `debug_assertions` is on in one and off in the
//! other, so a `#[cfg(debug_assertions)]` field, item or test is *there* for the unoptimised run and
//! gone for the optimised one. A lint about code that only exists in one of them can therefore only
//! fire in one of them, and a gate that runs the other profile is a gate that reports the code is
//! clean when it is not.
//!
//! That is not hypothetical. A `drop` of a guard whose only field is behind `debug_assertions` is a
//! `drop` of a type that implements `Drop` in the unoptimised build and of a plain reference in the
//! optimised one, so `clippy::drop_non_drop` is silent in debug and right in release. The same shape
//! produces an unused import: the only test that names it is a debug-build test.
//!
//! So both profiles run, over the whole workspace and every target in it, and `-D warnings` makes
//! either one fatal. The second is cheap — clippy emits metadata rather than machine code, and the
//! two runs cache separately and do not invalidate each other, so a repeat costs seconds.
//!
//! [`PROFILES`] is the whole of the wiring, and every clippy invocation in this xtask goes through
//! here, so no profile can be linted in one place and not the other.

use std::path::Path;

use crate::error::Result;
use crate::process;

/// The profiles every lint run covers, as the arguments that select them.
///
/// The empty slice is the default, unoptimised profile: cargo has no `--debug`, and naming the
/// absence of a flag is what keeps the two profiles one list rather than two code paths.
pub(crate) const PROFILES: [&[&str]; 2] = [&[], &["--release"]];

/// Lints every target of every workspace member, in both profiles.
pub(crate) fn run(root: &Path) -> Result<()> {
    for profile in PROFILES {
        each(root, profile, &["--workspace", "--all-targets"])?;
    }
    Ok(())
}

/// Lints one member's `target` test target with `feature` turned on, in both profiles.
///
/// A target behind a feature is invisible to the workspace run above, which turns no feature on, so
/// whoever owns such a target asks for it to be linted here.
pub(crate) fn feature_target(root: &Path, member: &str, feature: &str, target: &str) -> Result<()> {
    for profile in PROFILES {
        each(
            root,
            profile,
            &["-p", member, "--features", feature, "--test", target],
        )?;
    }
    Ok(())
}

/// One clippy invocation: the profile, what to lint, and warnings made fatal.
fn each(root: &Path, profile: &[&str], what: &[&str]) -> Result<()> {
    let mut args = vec!["clippy"];
    args.extend_from_slice(profile);
    args.extend_from_slice(what);
    args.extend_from_slice(&["--", "-D", "warnings"]);
    process::run(root, &process::cargo(), &args, &[])
}
