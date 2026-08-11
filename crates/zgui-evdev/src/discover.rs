//! Finding the devices on this machine.
//!
//! The kernel puts one node under `/dev/input` for every device it has, and a process reads the
//! ones it has permission to. Most of them belong to the `input` group, so a program outside that
//! group opens none, which is the ordinary case rather than a failure.
//!
//! # Opening what the walk lists
//!
//! This module answers paths. Where a descriptor comes from is the caller's own question, and it
//! has two answers: a program with the group opens the node itself, and a program on a seat asks
//! the session daemon, which opens the node and hands the descriptor over. One walk serves both,
//! and each caller builds its devices with [`Device::open`](crate::Device::open) or
//! [`Device::over`](crate::Device::over).

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Where the kernel puts input devices.
///
/// [`nodes`] walks this directory and [`Watch::new`](crate::Watch::new) watches it. A caller that
/// names it for itself — in a message that says where the devices came from, or in a test that
/// uses a directory of its own — names it through this constant, so the path is written down once.
pub const DIRECTORY: &str = "/dev/input";

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

/// Returns every event node under `/dev/input`, in the order the kernel numbers them.
///
/// A path here is one to try. Opening it is the caller's own step, and permission is the ordinary
/// refusal.
///
/// ```no_run
/// use zgui_evdev::{Device, Role};
///
/// let mut keyboards = Vec::new();
/// for node in zgui_evdev::nodes()? {
///     assert!(node.starts_with(zgui_evdev::DIRECTORY));
///     match Device::open(&node) {
///         Ok(device) if device.roles().contains(Role::Keyboard) => keyboards.push(device),
///         Ok(_other) => {}
///         // Permission is the ordinary refusal, and the walk carries on past it.
///         Err(refused) => eprintln!("{}: {refused}", node.display()),
///     }
/// }
/// # Ok::<(), zgui_evdev::Error>(())
/// ```
///
/// # Errors
///
/// Returns [`Error::Open`] when the directory cannot be read.
pub fn nodes() -> Result<Vec<PathBuf>> {
    nodes_in(DIRECTORY)
}

/// Returns every event node in `directory`, in the order the kernel numbers them.
///
/// `event2` comes before `event10`, where sorting by name puts them the other way round.
///
/// ```
/// // A directory that is not there is the one thing a walk fails on. A node that refuses to open
/// // is still listed, because opening it is the caller's step.
/// assert!(zgui_evdev::nodes_in("/dev/input/no-such-directory").is_err());
/// ```
///
/// # Errors
///
/// Returns [`Error::Open`] when the directory cannot be read.
pub fn nodes_in(directory: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    Ok(numbered_in(directory.as_ref())?
        .into_iter()
        .map(|(_, path)| path)
        .collect())
}

/// Returns every event node in `directory` with its number, in the order the kernel numbers them.
///
/// # Errors
///
/// Returns [`Error::Open`] when the directory cannot be read.
pub(crate) fn numbered_in(directory: &Path) -> Result<Vec<(u32, PathBuf)>> {
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

/// A path that could not be opened, and why.
#[derive(Debug)]
pub struct Skipped {
    /// The path that was tried.
    pub path: PathBuf,
    /// What it refused with. Permission is the ordinary case.
    pub reason: Error,
}

#[cfg(test)]
mod tests {
    //! Which entries a walk picks up, over a directory made here.
    //!
    //! No device is needed: the naming rule and the ordering are questions about a directory.

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

    /// The file names a walk answered, in the order it answered them.
    fn named(listed: &[PathBuf]) -> Vec<String> {
        listed
            .iter()
            .map(|path| {
                path.file_name()
                    .expect("a node has a name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[test]
    fn only_the_event_nodes_are_listed() {
        // `/dev/input` also holds `mice`, `mouse0`, `by-id` and `by-path`. `mouse0` is the one
        // that matters: it starts with the prefix and is a different interface, so a walk that
        // matched on the prefix alone would read a mouse through a protocol this crate has never
        // heard of.
        let root = Directory::new(
            "only_the_event_nodes_are_listed",
            &["event0", "mice", "mouse0", "js0"],
        );

        let listed = nodes_in(&root.0).expect("the directory reads");

        assert_eq!(
            named(&listed),
            ["event0"],
            "only the numbered event nodes are named at all"
        );
    }

    #[test]
    fn the_nodes_come_back_in_the_order_the_kernel_numbers_them() {
        // Sorted by name, `event10` comes before `event2`. The number is what the kernel means.
        let root = Directory::new(
            "the_nodes_come_back_in_the_order_the_kernel_numbers_them",
            &["event0", "event2", "event10", "event11"],
        );

        let listed = nodes_in(&root.0).expect("the directory reads");

        assert_eq!(named(&listed), ["event0", "event2", "event10", "event11"]);
    }

    #[test]
    fn every_node_is_named_by_the_path_it_is_opened_at() {
        // The walk answers paths, and a caller opens each one. So each entry is the directory it
        // was found in joined to the name, and that is the path a caller passes to `Device::open`
        // or hands to a session daemon.
        let root = Directory::new(
            "every_node_is_named_by_the_path_it_is_opened_at",
            &["event0", "event2", "event10", "mouse0", "by-id"],
        );

        let listed = nodes_in(&root.0).expect("the directory reads");

        assert_eq!(
            listed,
            [
                root.0.join("event0"),
                root.0.join("event2"),
                root.0.join("event10")
            ],
            "the numbered event nodes, in the kernel's own order"
        );
    }

    #[test]
    fn a_directory_that_cannot_be_read_is_the_one_thing_that_fails() {
        let missing = std::env::temp_dir().join("zgui-evdev-no-such-directory");
        let _ = std::fs::remove_dir_all(&missing);

        assert!(
            nodes_in(&missing).is_err(),
            "a walk of nothing is a failure, where a node that refuses to open is not"
        );
    }
}
