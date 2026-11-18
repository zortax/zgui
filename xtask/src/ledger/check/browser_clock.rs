//! Every portable crate reads its clocks through `web_time`.
//!
//! `std::time::Instant::now` and `std::time::SystemTime::now` compile for `wasm32-unknown-unknown`
//! and panic when they run. `web_time` exports the `std::time` types on a native target and the
//! browser's clocks on wasm32, so a crate that a browser build links names `web_time` instead.

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// The crates that only a native target builds, which keep `std::time`.
const NATIVE_ONLY: &[&str] = &[
    "zgui-platform-wayland",
    "zgui-platform-drm",
    "zgui-platform-winit",
    "zgui-drm",
    "zgui-seat",
    "zgui-libinput",
    "zgui-evdev",
    "zgui-xkb",
    "zgui-console",
    "zgui-tokio",
    "zgui-bench",
    "zgui-examples",
    "probe",
    "xtask",
];

/// The clock types that panic in a browser when read through `std::time`.
const CLOCKS: &[&str] = &["Instant", "SystemTime", "UNIX_EPOCH"];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    for member in &tree.members {
        if NATIVE_ONLY.contains(&member.name.as_str()) || member.is_spike() {
            continue;
        }
        let library = format!("{}/src/", member.rel_dir);
        for source in &member.sources {
            if !source.rel_path.starts_with(&library) {
                continue;
            }
            for (line, text) in source.text.lines().enumerate() {
                if let Some(clock) = named_clock(text) {
                    report.violation(
                        source.rel_path.clone(),
                        format!(
                            "line {}: `std::time::{clock}` panics in a browser; name \
                             `web_time::{clock}`, which is the same type on a native target",
                            line + 1
                        ),
                    );
                }
            }
        }
    }
    report
}

/// The clock type `text` takes from `std::time`, if it takes one.
///
/// Every `std::time::` on the line is read, and so is a `time::` inside a `std::{…}` group.
fn named_clock(text: &str) -> Option<&'static str> {
    let direct = text.split("std::time::").skip(1);
    let grouped = text
        .contains("std::{")
        .then(|| text.split("time::").skip(1))
        .into_iter()
        .flatten();
    direct.chain(grouped).find_map(clock_at)
}

/// The clock type at the start of `rest`, which follows a `time::`: a name or a group of names.
fn clock_at(rest: &str) -> Option<&'static str> {
    if rest.starts_with('*') {
        return Some("Instant");
    }
    if let Some(group) = rest.strip_prefix('{') {
        let group = group.split('}').next().unwrap_or(group);
        return CLOCKS.iter().copied().find(|clock| {
            group
                .split(',')
                .any(|name| name.trim().split(' ').next() == Some(clock))
        });
    }
    CLOCKS.iter().copied().find(|clock| {
        rest.strip_prefix(clock).is_some_and(|after| {
            !after.starts_with(|next: char| next.is_alphanumeric() || next == '_')
        })
    })
}

#[cfg(test)]
mod tests {
    use super::named_clock;

    #[test]
    fn a_clock_from_std_is_a_violation_in_each_spelling() {
        assert_eq!(named_clock("use std::time::Instant;"), Some("Instant"));
        assert_eq!(
            named_clock("use std::time::{Duration, SystemTime};"),
            Some("SystemTime")
        );
        assert_eq!(
            named_clock("let now = std::time::Instant::now();"),
            Some("Instant")
        );
    }

    #[test]
    fn a_clock_later_on_the_line_or_inside_a_group_of_std_is_found() {
        assert_eq!(
            named_clock("fn f(d: std::time::Duration) -> std::time::Instant {"),
            Some("Instant")
        );
        assert_eq!(
            named_clock("use std::{rc::Rc, time::Instant};"),
            Some("Instant")
        );
        assert_eq!(named_clock("use std::time::*;"), Some("Instant"));
        assert_eq!(named_clock("use std::{rc::Rc, time::Duration};"), None);
    }

    #[test]
    fn a_duration_and_the_portable_clock_are_not_violations() {
        assert_eq!(named_clock("use std::time::Duration;"), None);
        assert_eq!(named_clock("use std::time::{Duration};"), None);
        assert_eq!(named_clock("use web_time::{Duration, Instant};"), None);
    }
}
