//! Finding devices to test against, and saying so when there are none.
//!
//! `cargo xtask ledger ignored` forbids switching a test off, and states the alternative: a test
//! that needs a device looks for one, reports on standard error that it did not find one, and
//! returns. This module is that search. The refusal is then a fact about the machine, printed where
//! it happened, and it lasts as long as the machine stays that way.
//!
//! Most `/dev/input/event*` nodes belong to the `input` group, so the ordinary machine hands back
//! an empty list to a program run by an ordinary user. That is why the skipped nodes are printed
//! too: a run that asserts nothing should say why, in enough detail to act on.

#![cfg(target_os = "linux")]

use zgui_evdev::Device;

/// Returns every device on this machine that can be opened, or an empty list with the reasons
/// printed.
pub(crate) fn devices(test: &str) -> Vec<Device> {
    let found = match zgui_evdev::discover() {
        Ok(found) => found,
        Err(error) => {
            eprintln!("{test}: /dev/input cannot be read on this machine: {error}");
            return Vec::new();
        }
    };

    for skipped in &found.skipped {
        println!("skipped {}: {}", skipped.path.display(), skipped.reason);
    }
    if found.opened.is_empty() {
        eprintln!(
            "{test}: no input device on this machine can be opened, so nothing was asserted; \
             add this user to the `input` group to run it"
        );
    }
    found.opened
}
