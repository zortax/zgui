//! The unsafe ledger.
//!
//! `#![forbid(unsafe_code)]` on every crate outside the allowlist, and every `unsafe impl Sync`
//! or `unsafe impl Send` in the workspace stated with its reason beside it. An allowlist entry
//! names what the crate needs unsafe *for*; a crate that needs unsafe for something else needs
//! a new entry and the review that comes with it.

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// The attribute every crate outside the allowlist carries.
const FORBID: &str = "#![forbid(unsafe_code)]";

/// The comment that has to sit directly above a hand-written `Sync`/`Send` promise.
const SAFETY: &str = "// SAFETY:";

/// Crates permitted to contain unsafe code, each with the reason it is permitted.
const ALLOWLIST: &[(&str, &str)] = &[
    (
        "zgui-arena",
        "chunk initialisation hands out address-stable references",
    ),
    ("zgui-dom", "the cell discipline and the mutation cell"),
    (
        "zgui-geom",
        "plain-old-data impls for the types that cross to the GPU",
    ),
    ("zgui-render-wgpu", "GPU resource handling"),
    (
        "zgui-render-vector-vello",
        "creating a pipeline cache from a stored blob",
    ),
    (
        "zgui-platform-winit",
        "the Wayland clipboard's unsafe constructor",
    ),
    (
        "zgui-platform-wayland",
        "borrowing the compositor's display and surface pointers as window handles",
    ),
    ("zgui-drm", "the ioctls, and the mapping of a dumb buffer"),
    (
        "zgui-evdev",
        "the ioctls, and reading an event record out of a byte buffer",
    ),
    (
        "zgui-platform-drm",
        "the two `borrow_raw` calls that report a surface's DRM handles",
    ),
];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    for member in &tree.members {
        let allowed = ALLOWLIST.iter().any(|(name, _)| *name == member.name);
        if !allowed {
            match member.crate_root() {
                Some(file) if file.text.contains(FORBID) => {}
                Some(file) => report.violation(
                    file.rel_path.clone(),
                    format!(
                        "`{}` is not on the unsafe allowlist, so its crate root must carry `{FORBID}`",
                        member.name
                    ),
                ),
                None => report.violation(
                    member.rel_dir.clone(),
                    format!(
                        "`{}` has no src/lib.rs or src/main.rs to carry `{FORBID}`",
                        member.name
                    ),
                ),
            }
        }

        for file in &member.sources {
            for (number, line) in unsafe_impls(&file.text) {
                let preceding: Vec<&str> = file.text.lines().take(number).collect();
                let has_reason = preceding
                    .iter()
                    .rev()
                    .map(|line| line.trim_start())
                    .take_while(|line| line.starts_with("//"))
                    .any(|line| line.starts_with(SAFETY));
                if !has_reason {
                    report.violation(
                        format!("{}:{}", file.rel_path, number + 1),
                        format!("`{line}` must state its reason in a `{SAFETY}` comment directly above it"),
                    );
                }
            }
        }
    }
    report
}

/// Every `unsafe impl Sync`/`unsafe impl Send` line, as a zero-based line number and its text.
fn unsafe_impls(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim_start();
            trimmed.starts_with("unsafe impl")
                && (trimmed.contains("Sync") || trimmed.contains("Send"))
        })
        .map(|(number, line)| (number, line.trim().to_owned()))
        .collect()
}
