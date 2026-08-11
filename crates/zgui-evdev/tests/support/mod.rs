//! Finding devices to test against, and what the kernel says each one is.
//!
//! `cargo xtask ledger ignored` forbids switching a test off, and states the alternative: a test
//! that needs a device looks for one, reports on standard error that it did not find one, and
//! returns. This module is that search. The refusal is then a fact about the machine, printed where
//! it happened, and it lasts as long as the machine stays that way.
//!
//! Most `/dev/input/event*` nodes belong to the `input` group, so the ordinary machine hands back
//! an empty list to a program run by an ordinary user, and the message says which group to join.
//!
//! # Where the answers come from
//!
//! Which nodes exist and which of them this process may open is asked of `/dev/input` through
//! `std::fs`. What each device is comes from `/sys/class/input`, which the kernel writes from the
//! same `input_dev` the ioctls read. Neither question reaches `zgui_evdev`.
//!
//! That separation is the point of this module. A helper that asked the crate whether a device
//! opens would send every test in the binary into the silent arm exactly when the crate stopped
//! opening devices, and print a message blaming the machine for it. So a node listed here is a node
//! [`devices`] **asserts** [`Device::open`] takes, and [`published`] is what the kernel says that
//! device is, for a test to hold the crate's own answer against.

#![cfg(target_os = "linux")]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use zgui_evdev::Device;

/// Where the kernel puts input devices.
///
/// Written out here instead of taken from [`zgui_evdev::DIRECTORY`], for the reason above: this
/// module asks the machine, and a wrong constant in the crate would otherwise decide that the
/// machine has no devices.
const NODES: &str = "/dev/input";

/// Where the kernel publishes what each device is.
const PUBLISHED: &str = "/sys/class/input";

/// The name every event node starts with.
const PREFIX: &str = "event";

/// Returns the `event*` nodes this process can open, in the order the kernel numbers them.
///
/// Each candidate is opened read-only, the mode [`Device::open`] asks for. So a node in this list
/// is a node that crate is expected to take, and a machine that answers an empty list is a machine
/// where nothing about a real device can be asserted at all.
pub(crate) fn openable_nodes() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(NODES) else {
        return Vec::new();
    };

    let mut nodes: Vec<(u32, PathBuf)> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| {
            let number = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix(PREFIX))
                .and_then(|number| number.parse().ok())?;
            Some((number, path))
        })
        .filter(|(_, path)| fs::File::open(path).is_ok())
        .collect();
    nodes.sort();
    nodes.into_iter().map(|(_, path)| path).collect()
}

/// Returns every device on this machine that can be opened, or an empty list with the reason
/// printed.
///
/// Every node [`openable_nodes`] answers is opened here, and a refusal **fails**. The machine has
/// already said that this process may open the node, so a device that will not open is this crate
/// declining one it was handed.
pub(crate) fn devices(test: &str) -> Vec<Device> {
    let nodes = openable_nodes();
    if nodes.is_empty() {
        eprintln!(
            "{test}: no `event*` under /dev/input opens for this process, so nothing was \
             asserted; add this user to the group the nodes belong to — `input` on most machines \
             — to run it"
        );
        return Vec::new();
    }

    nodes
        .iter()
        .map(|path| {
            Device::open(path).unwrap_or_else(|error| {
                panic!(
                    "this process opens {} for itself, so this crate takes it: {error}",
                    path.display()
                )
            })
        })
        .collect()
}

/// What the kernel publishes about the device behind one node.
///
/// The codes are the kernel's own numbers, and a [`zgui_evdev::Bitmap`] answers those through
/// [`zgui_evdev::Code::raw`], so the two sets compare directly.
pub(crate) struct Published {
    /// What the device calls itself.
    pub(crate) name: String,
    /// Which bus it is on.
    pub(crate) bus: u16,
    /// The vendor's number.
    pub(crate) vendor: u16,
    /// The product's number.
    pub(crate) product: u16,
    /// The version the device reports.
    pub(crate) version: u16,
    /// Which event types it emits.
    pub(crate) types: BTreeSet<u16>,
    /// Which keys and buttons it has.
    pub(crate) keys: BTreeSet<u16>,
    /// Which relative axes it has.
    pub(crate) relative: BTreeSet<u16>,
    /// Which absolute axes it has.
    pub(crate) absolute: BTreeSet<u16>,
}

/// Returns what the kernel publishes about the node at `path`, or nothing with the reason printed.
///
/// `/sys/class/input/eventN/device` is the `input_dev` behind the node, and the files under it are
/// written by the same driver the ioctls answer from. So this is one fact read two ways, and a
/// test that holds the two against each other is holding this crate's requests up against
/// something outside it.
///
/// A machine with no `sysfs`, or one whose kernel publishes none of this, answers nothing and the
/// caller reports it.
pub(crate) fn published(test: &str, path: &Path) -> Option<Published> {
    let node = path.file_name()?;
    let directory = Path::new(PUBLISHED).join(node).join("device");
    let read = |name: &str| -> Option<String> {
        match fs::read_to_string(directory.join(name)) {
            Ok(text) => Some(text.trim().to_owned()),
            Err(error) => {
                eprintln!(
                    "{test}: the kernel publishes no {name} under {}, so what {} is was read from \
                     the device alone: {error}",
                    directory.display(),
                    path.display()
                );
                None
            }
        }
    };

    Some(Published {
        name: read("name")?,
        bus: number(&read("id/bustype")?),
        vendor: number(&read("id/vendor")?),
        product: number(&read("id/product")?),
        version: number(&read("id/version")?),
        types: codes(&read("capabilities/ev")?),
        keys: codes(&read("capabilities/key")?),
        relative: codes(&read("capabilities/rel")?),
        absolute: codes(&read("capabilities/abs")?),
    })
}

/// Reads one of the four numbers `/sys/class/input/*/device/id` holds.
///
/// The kernel writes each as four hexadecimal digits with no prefix.
fn number(text: &str) -> u16 {
    u16::from_str_radix(text, 16).unwrap_or_else(|error| {
        panic!("the kernel writes an id as hexadecimal, and it wrote {text:?}: {error}")
    })
}

/// Reads the codes one published capability map names.
///
/// The kernel prints a bitmap as one hexadecimal word per machine word, **most significant first**
/// and separated by spaces, with the word holding code zero written last. So the groups are
/// counted from the right.
fn codes(text: &str) -> BTreeSet<u16> {
    let width = usize::BITS;
    text.split_whitespace()
        .rev()
        .enumerate()
        .flat_map(|(group, word)| {
            let bits = u64::from_str_radix(word, 16).unwrap_or_else(|error| {
                panic!(
                    "the kernel prints a bitmap as hexadecimal, and it printed {word:?}: {error}"
                )
            });
            let base = u32::try_from(group).unwrap_or(0) * width;
            (0..width.min(u64::BITS))
                .filter(move |bit| bits >> bit & 1 == 1)
                .filter_map(move |bit| u16::try_from(base + bit).ok())
        })
        .collect()
}
