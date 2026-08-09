//! Finding the devices on this machine.
//!
//! The kernel puts one node under `/dev/input` for every device it has, and a process reads the
//! ones it has permission to. Most of them belong to the `input` group, so a program outside that
//! group opens none, which is the ordinary case rather than a failure. A discovery therefore
//! reports what it skipped beside what it opened.

use std::path::{Path, PathBuf};

use crate::device::Device;
use crate::error::{Error, Result};

/// Where the kernel puts input devices.
pub(crate) const DIRECTORY: &str = "/dev/input";

/// The prefix an event node's name has.
const PREFIX: &str = "event";

/// Returns the kernel's number for the node called `name`, where that name is an event node.
///
/// `/dev/input` also holds `mice`, `mouse0`, `js0`, `by-id` and `by-path`. `mouse0` is the one that
/// matters: it starts with the prefix and is a different interface, so a rule written against the
/// prefix alone would read a mouse through a protocol this crate has never heard of.
///
/// The number carries the kernel's own order, which the name does not: `event2` comes before
/// `event10`, and sorting by name puts them the other way round.
pub(crate) fn node_number(name: &str) -> Option<u32> {
    name.strip_prefix(PREFIX)?.parse().ok()
}

/// Returns every event node in `directory` with its number, in the order the kernel numbers them.
///
/// # Errors
///
/// Returns [`Error::Open`] when the directory cannot be read.
pub(crate) fn nodes_in(directory: &Path) -> Result<Vec<(u32, PathBuf)>> {
    let entries = std::fs::read_dir(directory).map_err(|source| Error::Open {
        path: directory.to_owned(),
        source,
    })?;

    let mut nodes: Vec<(u32, PathBuf)> = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let number = node_number(path.file_name()?.to_str()?)?;
            Some((number, path))
        })
        .collect();
    nodes.sort();
    Ok(nodes)
}

/// A node that could not be opened, and why.
#[derive(Debug)]
pub struct Skipped {
    /// The node that was tried.
    pub path: PathBuf,
    /// What it refused with. Permission is the ordinary case.
    pub reason: Error,
}

/// What a walk of the directory found.
#[derive(Debug)]
pub struct Discovery {
    /// The devices that opened, in node order.
    pub opened: Vec<Device>,
    /// The nodes that did not, with the reason each gave.
    pub skipped: Vec<Skipped>,
}

/// Opens every device under `/dev/input` that can be opened.
///
/// # Errors
///
/// Returns [`Error::Open`] when the directory itself cannot be read. A node that cannot be opened
/// is not an error: it lands in [`Discovery::skipped`] with its reason, because a machine where
/// half the devices belong to another group is the ordinary machine.
pub fn discover() -> Result<Discovery> {
    discover_in(DIRECTORY)
}

/// Opens every device under `directory` that can be opened.
///
/// The nodes come back in the order the kernel numbers them: `event2` before `event10`, which the
/// name alone does not give.
///
/// # Errors
///
/// Returns [`Error::Open`] when the directory cannot be read.
pub fn discover_in(directory: impl AsRef<Path>) -> Result<Discovery> {
    let nodes = nodes_in(directory.as_ref())?;

    let mut discovery = Discovery {
        opened: Vec::new(),
        skipped: Vec::new(),
    };
    for (_, path) in nodes {
        match Device::open(&path) {
            Ok(device) => discovery.opened.push(device),
            Err(reason) => discovery.skipped.push(Skipped { path, reason }),
        }
    }
    Ok(discovery)
}

#[cfg(test)]
mod tests {
    //! Which entries a walk picks up, over a directory made here.
    //!
    //! No device is needed: the naming rule and the ordering are questions about a directory. The
    //! files below are ordinary files, so every one of them fails to open as an input device, and
    //! that is what makes them a test of the skipping.

    use super::*;

    /// A directory holding one empty file per name, removed when it goes out of scope.
    ///
    /// Named after the test that asked for it, so two tests running at once do not share one.
    struct Directory(PathBuf);

    impl Directory {
        /// Makes the directory and the files in it.
        fn new(test: &str, names: &[&str]) -> Self {
            let root =
                std::env::temp_dir().join(format!("zgui-evdev-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("the directory is made");
            for name in names {
                std::fs::write(root.join(name), []).expect("the file is made");
            }
            Self(root)
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The file names a walk reported skipping, in the order it reported them.
    fn skipped(found: &Discovery) -> Vec<String> {
        found
            .skipped
            .iter()
            .map(|skipped| {
                skipped
                    .path
                    .file_name()
                    .expect("a node has a name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[test]
    fn only_the_event_nodes_are_tried() {
        // `/dev/input` also holds `mice`, `mouse0`, `by-id` and `by-path`. `mouse0` is the one
        // that matters: it starts with the prefix and is a different interface, so a walk that
        // matched on the prefix alone would read a mouse through a protocol this crate has never
        // heard of.
        let root = Directory::new(
            "only_the_event_nodes_are_tried",
            &["event0", "mice", "mouse0", "js0"],
        );

        let found = discover_in(&root.0).expect("the directory reads");

        assert!(found.opened.is_empty(), "an empty file is no device");
        assert_eq!(
            skipped(&found),
            ["event0"],
            "only the numbered event nodes are tried at all"
        );
    }

    #[test]
    fn the_nodes_come_back_in_the_order_the_kernel_numbers_them() {
        // Sorted by name, `event10` comes before `event2`. The number is what the kernel means.
        let root = Directory::new(
            "the_nodes_come_back_in_the_order_the_kernel_numbers_them",
            &["event0", "event2", "event10", "event11"],
        );

        let found = discover_in(&root.0).expect("the directory reads");

        assert_eq!(skipped(&found), ["event0", "event2", "event10", "event11"]);
    }

    #[test]
    fn a_node_that_will_not_open_is_reported_with_its_reason() {
        let root = Directory::new(
            "a_node_that_will_not_open_is_reported_with_its_reason",
            &["event0"],
        );

        let found = discover_in(&root.0).expect("the directory reads");

        assert_eq!(found.skipped.len(), 1);
        assert!(
            !found.skipped[0].reason.to_string().is_empty(),
            "the reason is what a person acts on, so it says something"
        );
    }

    #[test]
    fn a_directory_that_cannot_be_read_is_the_one_thing_that_fails() {
        let missing = std::env::temp_dir().join("zgui-evdev-no-such-directory");
        let _ = std::fs::remove_dir_all(&missing);

        assert!(
            discover_in(&missing).is_err(),
            "a walk of nothing is a failure, where a device that refuses is not"
        );
    }
}
