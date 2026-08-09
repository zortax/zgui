//! Hearing about the devices that arrive while a program runs.
//!
//! [`discover`](mod@crate::discover) reads `/dev/input` once. A keyboard plugged in afterwards puts
//! a new node in that directory and nothing says so, so this watches the same directory and reports
//! the event nodes that may now be openable. The watch sits beside the walk because it asks the
//! same question a second way, and both apply the same rule about what an event node is called.
//!
//! # When a node can be opened
//!
//! The kernel creates `/dev/input/eventN` and **udev sets its owner and its mode afterwards**. A
//! watch that acts on the creation alone opens the node while it is still owned by root and
//! readable by nobody else, gets `EACCES`, and then never hears about that device again — so the
//! keyboard somebody just plugged in stays dead for the rest of the program. `IN_ATTRIB` says udev
//! has finished, so it is watched beside `IN_CREATE`.
//!
//! The cost is that one hotplug names the same node twice. Taking it once is the caller's to get
//! right; nothing here can, because a node removed and made again under the same name is a
//! different device with the same path.
//!
//! # Removals
//!
//! A node going away is not watched. The kernel makes an open descriptor readable the moment its
//! device is gone and answers `ENODEV` for every read after that, so a loop already parked on the
//! device learns it faster than a watch could tell it.

use std::collections::BTreeSet;
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};

use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::fs::inotify::{self, CreateFlags, ReadFlags, WatchFlags};
use rustix::io::Errno;

use crate::discover::{DIRECTORY, node_number, nodes_in};
use crate::error::{Error, Result};

/// What the kernel is asked to report.
///
/// See the module documentation for why the ownership change is asked for as well as the creation.
const ASKED: WatchFlags = WatchFlags::CREATE.union(WatchFlags::ATTRIB);

/// The same two reports, in the type they read back in.
///
/// The kernel uses one set of bits for both and `rustix` gives each direction its own type, so the
/// pair is written twice. They must name the same two things: a bit asked for and never acted on
/// wakes the loop for nothing, and a bit acted on and never asked for never arrives.
const ARRIVALS: ReadFlags = ReadFlags::CREATE.union(ReadFlags::ATTRIB);

/// How many bytes one read takes at a time.
///
/// A report is a fixed header and a name of at most `NAME_MAX` bytes, so this holds several whole
/// ones. The length only decides how many reads a burst costs: reading goes on until the kernel
/// says there is nothing left.
const BUFFER: usize = 4096;

/// Returns `true` if this report says a node may now be openable.
const fn announces_a_node(flags: ReadFlags) -> bool {
    flags.intersects(ARRIVALS)
}

/// Returns `true` if the kernel discarded reports.
///
/// The queue has a fixed length and the kernel discards everything past it, putting `IN_Q_OVERFLOW`
/// in its place. What was discarded cannot be known, so the answer is to read the whole directory
/// again and name every node in it. A device the caller already holds is one it takes no second
/// time, and a device it never heard about is one nobody can type on.
const fn overflowed(flags: ReadFlags) -> bool {
    flags.contains(ReadFlags::QUEUE_OVERFLOW)
}

/// A watch on the directory the kernel puts input devices in.
///
/// It holds one descriptor and hands it out through [`AsFd`], so a loop parks on it beside the
/// devices themselves. That loop calls [`Watch::arrived`] when the descriptor becomes readable.
#[derive(Debug)]
pub struct Watch {
    /// The inotify object.
    ///
    /// Non-blocking, because the loop that parks on it reads it dry every turn and a blocking read
    /// with nothing left would stop the program instead.
    inotify: OwnedFd,
    /// The directory being watched.
    ///
    /// A report carries a bare name, so this is what turns one into a path a caller can open.
    directory: PathBuf,
}

impl Watch {
    /// Watches `/dev/input` for the devices that arrive.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Watch`] when the inotify object cannot be made or the directory cannot be
    /// watched.
    pub fn new() -> Result<Self> {
        Self::new_in(DIRECTORY)
    }

    /// Watches `directory` for the devices that arrive.
    ///
    /// ```
    /// use zgui_evdev::Watch;
    ///
    /// // A directory the machine does not have cannot be watched, and the refusal names it.
    /// let refused = Watch::new_in("/dev/input/no-such-directory").expect_err("there is nothing");
    ///
    /// assert!(refused.to_string().contains("no-such-directory"));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Watch`] when the inotify object cannot be made or the directory cannot be
    /// watched.
    pub fn new_in(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref().to_owned();
        let failed = |errno: Errno| Error::Watch {
            path: directory.clone(),
            source: errno.into(),
        };
        let inotify =
            inotify::init(CreateFlags::CLOEXEC | CreateFlags::NONBLOCK).map_err(failed)?;
        inotify::add_watch(&inotify, &directory, ASKED).map_err(failed)?;
        Ok(Self { inotify, directory })
    }

    /// Returns the directory this watches.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Returns every event node reported since the last call, in the order the kernel numbers them.
    ///
    /// Each node is named once however many reports it drew, so the two a hotplug produces are one
    /// path. The same node reported again on a later call is named again, because a node removed
    /// and made once more under that name is a different device.
    ///
    /// A path this reports is one that may be openable rather than one that is. The ordinary
    /// answer to the creation of a node udev has not finished with is `EACCES`, and the report that
    /// says it has finished arrives afterwards.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Watch`] when the reports cannot be read, and [`Error::Open`] when the
    /// kernel dropped reports and the directory then cannot be read to make up for them.
    pub fn arrived(&self) -> Result<Vec<PathBuf>> {
        // Ordered by the kernel's own number and deduplicated: the order `discover` answers in, and
        // the reason the two reports of one hotplug come back as one path.
        let mut nodes: BTreeSet<(u32, PathBuf)> = BTreeSet::new();
        let mut dropped = false;
        let mut buffer = [MaybeUninit::<u8>::uninit(); BUFFER];
        let mut reports = inotify::Reader::new(&self.inotify, &mut buffer);
        loop {
            let report = match reports.next() {
                Ok(report) => report,
                // Nothing left to read, which is how every drain ends. A signal that cut the read
                // short is the same answer: the reports stay queued, the descriptor stays readable,
                // and the next turn reads them.
                Err(Errno::AGAIN | Errno::INTR) => break,
                Err(errno) => {
                    return Err(Error::Watch {
                        path: self.directory.clone(),
                        source: errno.into(),
                    });
                }
            };
            dropped |= overflowed(report.events());
            if !announces_a_node(report.events()) {
                continue;
            }
            let Some(name) = report.file_name().and_then(|name| name.to_str().ok()) else {
                continue;
            };
            if let Some(number) = node_number(name) {
                nodes.insert((number, self.directory.join(name)));
            }
        }
        if dropped {
            nodes.extend(nodes_in(&self.directory)?);
        }
        Ok(nodes.into_iter().map(|(_, path)| path).collect())
    }
}

/// The descriptor a loop parks on.
impl AsFd for Watch {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.inotify.as_fd()
    }
}

#[cfg(test)]
mod tests {
    //! Which reports are acted on, and which names, over a directory made here.
    //!
    //! No device and no privilege. The decisions are pure, and the watch itself runs against a
    //! directory in the temporary area, where an ordinary file stands in for a node: what is being
    //! asserted is which reports reach a caller, and a file draws the same ones a device node does.

    use super::*;

    /// A directory that a test makes nodes in, removed when it goes out of scope.
    ///
    /// Named after the test that asked for it, so two tests running at once do not share one.
    struct Scratch(PathBuf);

    impl Scratch {
        /// An empty directory of its own.
        fn new(test: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("zgui-evdev-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("the directory is made");
            Self(root)
        }

        /// Makes a file called `name`, which draws the report a new node draws.
        fn create(&self, name: &str) {
            std::fs::write(self.0.join(name), []).expect("the file is made");
        }

        /// Changes the mode of `name`, drawing the report udev draws when it takes ownership.
        fn chmod(&self, name: &str) {
            let path = self.0.join(name);
            let mode = std::fs::metadata(&path)
                .expect("the file is there")
                .permissions();
            std::fs::set_permissions(&path, mode).expect("the mode is set");
        }

        /// Removes `name`.
        fn remove(&self, name: &str) {
            std::fs::remove_file(self.0.join(name)).expect("the file goes");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The file names a watch reported, in the order it reported them.
    fn names(reported: &[PathBuf]) -> Vec<String> {
        reported
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
    fn a_node_is_acted_on_when_it_is_made_and_again_when_udev_takes_it() {
        // The whole reason this watches two things. The kernel makes the node and udev sets its
        // ownership afterwards, so a watch that acted on the creation alone would open a node that
        // is still `root:root 0600`, get `EACCES`, and never hear about that device again.
        assert!(announces_a_node(ReadFlags::CREATE));
        assert!(announces_a_node(ReadFlags::ATTRIB));
        assert!(
            announces_a_node(ReadFlags::CREATE | ReadFlags::ISDIR),
            "and a report carries more than one bit"
        );
    }

    #[test]
    fn a_node_that_went_is_no_arrival() {
        // A device that goes is learnt from its own descriptor, which answers `ENODEV` and stays
        // readable. Acting on the removal here would try to open a node that is gone.
        assert!(!announces_a_node(ReadFlags::DELETE));
        assert!(!announces_a_node(ReadFlags::MODIFY));
        assert!(!announces_a_node(ReadFlags::default()));
    }

    #[test]
    fn the_two_directions_name_the_same_two_reports() {
        // The kernel uses one set of bits and `rustix` gives each direction its own type, so the
        // pair is written twice. A bit asked for and never acted on wakes a loop for nothing; a bit
        // acted on and never asked for never arrives at all.
        assert_eq!(ASKED.bits(), ARRIVALS.bits());
    }

    #[test]
    fn a_dropped_queue_asks_for_the_directory_to_be_read_again() {
        assert!(overflowed(ReadFlags::QUEUE_OVERFLOW));
        assert!(!overflowed(ReadFlags::CREATE));
        assert!(
            !announces_a_node(ReadFlags::QUEUE_OVERFLOW),
            "and it names no node of its own: it carries no name at all"
        );
    }

    /// How many reports the kernel keeps before it starts discarding them.
    ///
    /// Read rather than assumed: it is a tunable, and a test that wrote the usual 16384 in would
    /// assert nothing on a machine where somebody had raised it.
    fn queue_length() -> Option<usize> {
        std::fs::read_to_string("/proc/sys/fs/inotify/max_queued_events")
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    #[test]
    fn a_report_the_kernel_dropped_is_made_up_for_by_reading_the_directory() {
        // The queue has a fixed length and everything past it is discarded, so a burst larger than
        // the queue loses nodes that nothing would ever name again — and a device this backend
        // never heard of is one nobody can type on. What was discarded cannot be known, so every
        // node in the directory is named instead.
        let Some(queue) = queue_length() else {
            eprintln!(
                "a_report_the_kernel_dropped_is_made_up_for_by_reading_the_directory: \
                 /proc/sys/fs/inotify/max_queued_events cannot be read on this machine, so how \
                 many reports it takes to fill the queue is unknown and nothing was asserted; \
                 mount /proc to run it"
            );
            return;
        };
        let root =
            Scratch::new("a_report_the_kernel_dropped_is_made_up_for_by_reading_the_directory");
        let watch = Watch::new_in(&root.0).expect("the directory can be watched");

        // One report each, two past the length the kernel keeps: the first fills the queue and the
        // second is the one it discards.
        let last = format!("event{}", queue + 1);
        for number in 0..=queue + 1 {
            root.create(&format!("event{number}"));
        }

        let reported = names(&watch.arrived().expect("the reports read"));
        assert!(
            reported.contains(&last),
            "{last} was made and its report was discarded, so the directory is what names it"
        );
        assert_eq!(
            reported.len(),
            queue + 2,
            "and every node in the directory is named once"
        );
    }

    #[test]
    fn a_node_that_arrives_is_reported_with_the_path_it_can_be_opened_at() {
        let root =
            Scratch::new("a_node_that_arrives_is_reported_with_the_path_it_can_be_opened_at");
        let watch = Watch::new_in(&root.0).expect("the directory can be watched");

        root.create("event7");

        let reported = watch.arrived().expect("the reports read");
        assert_eq!(
            reported,
            [root.0.join("event7")],
            "the caller opens what this names, so it is a whole path"
        );
        assert_eq!(watch.directory(), root.0);
    }

    #[test]
    fn the_creation_and_the_ownership_change_of_one_node_are_one_path() {
        // What one hotplug looks like when both reports are read in the same turn. A caller told
        // twice would open the node twice and ask the kernel for a grab it already holds.
        let root = Scratch::new("the_creation_and_the_ownership_change_of_one_node_are_one_path");
        let watch = Watch::new_in(&root.0).expect("the directory can be watched");

        root.create("event7");
        root.chmod("event7");

        assert_eq!(
            names(&watch.arrived().expect("the reports read")),
            ["event7"]
        );
    }

    #[test]
    fn the_ownership_change_is_reported_on_its_own_when_it_arrives_in_a_later_turn() {
        // The ordinary case on a machine that reads fast enough: the creation wakes the loop, the
        // node is still `root:root`, and the report that says udev has finished comes next.
        let root = Scratch::new(
            "the_ownership_change_is_reported_on_its_own_when_it_arrives_in_a_later_turn",
        );
        let watch = Watch::new_in(&root.0).expect("the directory can be watched");

        root.create("event7");
        assert_eq!(
            names(&watch.arrived().expect("the reports read")),
            ["event7"]
        );

        root.chmod("event7");

        assert_eq!(
            names(&watch.arrived().expect("the reports read")),
            ["event7"],
            "the same node again, because the first answer may have been `EACCES`"
        );
    }

    #[test]
    fn only_the_event_nodes_are_reported() {
        // The same rule the walk applies. `mouse0` is the one that matters: it starts with the
        // prefix and is a different interface, so a watch that matched the prefix alone would hand
        // a caller a mouse to read through a protocol this crate has never heard of.
        let root = Scratch::new("only_the_event_nodes_are_reported");
        let watch = Watch::new_in(&root.0).expect("the directory can be watched");

        root.create("mice");
        root.create("mouse0");
        root.create("js0");
        root.create("event0");

        assert_eq!(
            names(&watch.arrived().expect("the reports read")),
            ["event0"]
        );
    }

    #[test]
    fn the_nodes_come_back_in_the_order_the_kernel_numbers_them() {
        // Sorted by name, `event10` comes before `event2`. The number is what the kernel means.
        let root = Scratch::new("the_nodes_come_back_in_the_order_the_kernel_numbers_them");
        let watch = Watch::new_in(&root.0).expect("the directory can be watched");

        for name in ["event10", "event2", "event0"] {
            root.create(name);
        }

        assert_eq!(
            names(&watch.arrived().expect("the reports read")),
            ["event0", "event2", "event10"]
        );
    }

    #[test]
    fn a_node_that_goes_is_reported_by_nothing() {
        let root = Scratch::new("a_node_that_goes_is_reported_by_nothing");
        let watch = Watch::new_in(&root.0).expect("the directory can be watched");
        root.create("event0");
        let _ = watch.arrived().expect("the reports read");

        root.remove("event0");

        assert!(
            watch.arrived().expect("the reports read").is_empty(),
            "a device that goes is learnt from its own descriptor"
        );
    }

    #[test]
    fn a_watch_with_nothing_to_report_answers_at_once() {
        // Non-blocking, so a loop can read this dry every turn. A blocking read here would stop the
        // whole program on the first turn nothing was plugged in.
        let root = Scratch::new("a_watch_with_nothing_to_report_answers_at_once");
        let watch = Watch::new_in(&root.0).expect("the directory can be watched");

        assert!(watch.arrived().expect("the reports read").is_empty());
    }

    #[test]
    fn a_directory_that_is_not_there_cannot_be_watched() {
        let missing = std::env::temp_dir().join("zgui-evdev-no-such-directory-to-watch");
        let _ = std::fs::remove_dir_all(&missing);

        let refusal = Watch::new_in(&missing).expect_err("there is nothing to watch");

        assert!(
            refusal.to_string().contains("cannot watch"),
            "the reason is what a person acts on: {refusal}"
        );
    }
}
