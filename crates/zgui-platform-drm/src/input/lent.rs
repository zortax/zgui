//! Where libinput's devices come from.
//!
//! libinput opens nothing. It asks its caller for a descriptor and hands it back when it is done,
//! and this module answers. It answers out of [`crate::session`], so a seated run reaches a
//! keyboard it is in no group for, and a direct run opens the node the way it always did.
//!
//! # The grab
//!
//! libinput takes no exclusive grab of its own, so the grab is made here, as the node is opened.
//! Without one, every keystroke also reaches the shell behind the console, so somebody typing
//! `reboot` into a text field leaves it on the command line for the shell to run when the program
//! exits. `libinput debug-events --grab` makes its grab in exactly this call for exactly this
//! reason.
//!
//! # The two descriptors
//!
//! The session hands over a [`zgui_evdev::Device`], which carries the grab and takes it off again
//! when it is dropped. libinput needs a descriptor it owns, because it closes what it is given, so
//! it is given a copy. The two name one open file description, so the grab covers both and closing
//! either leaves the other working.
//!
//! # The path
//!
//! The session takes a device back by path, and `close_restricted` is handed a descriptor and
//! nothing else. So [`Held`] files each device under the descriptor libinput was given, and the
//! path comes back out of it.

// Nothing a running program reaches gets here yet: `through` is the only caller, and nothing
// constructs a `Through` yet. The allow comes off with the milestone that gives `Seat` a second
// source. It is scoped to this module rather than turned off in the manifest, so the rest of the
// crate keeps reporting what it does not use.
#![allow(dead_code)]

use std::os::fd::{AsFd, AsRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::session::{Session, Unopened};

/// One device this process opened for libinput.
#[derive(Debug)]
pub(crate) struct Held {
    /// The descriptor libinput was given, and hands back.
    given: RawFd,
    /// The node it was opened at, which the session takes back.
    path: PathBuf,
    /// The device, which holds the grab until it is dropped.
    device: zgui_evdev::Device,
}

impl Held {
    /// Returns the node this device was opened at.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

/// The session, lent to libinput for one call.
///
/// libinput asks for a device from inside a call its caller made, so the session is borrowed for
/// that call and given up again when it returns. What is held between calls is [`Held`], which
/// belongs to whoever is reading libinput.
pub(crate) struct Lent<'a> {
    /// Where a device is asked for, and given back to.
    session: &'a mut Session,
    /// Every device that has been opened and not yet handed back.
    held: &'a mut Vec<Held>,
}

impl<'a> Lent<'a> {
    /// The session and the devices it has open.
    pub(crate) fn new(session: &'a mut Session, held: &'a mut Vec<Held>) -> Self {
        Self { session, held }
    }
}

impl zgui_libinput::Files for Lent<'_> {
    /// Opens one node through the session, grabs it, and hands libinput a copy of the descriptor.
    ///
    /// Every refusal is a number libinput puts in its own log and acts on in no other way, so what
    /// they say apart is what this crate logs.
    fn open(&mut self, path: &Path, _flags: i32) -> Result<OwnedFd, i32> {
        let mut device = match self.session.open_input(path) {
            Ok(device) => device,
            // A session waiting for its terminal is handed every node revoked. That is the state
            // rather than a fault: the device is this run's the moment somebody switches to this
            // terminal, and the resume is what opens it.
            Err(Unopened::NotYet(_)) => return Err(ENODEV),
            Err(Unopened::Refused(_)) => return Err(EACCES),
        };

        // Before the copy, so that the copy libinput reads is already grabbed. A device that
        // refuses the grab is still read: what that costs is the console behind this program
        // hearing every keystroke, which is worse than not reading the device at all only when
        // there is another device to read.
        if let Err(error) = device.grab() {
            warn!(
                target: "zgui::platform",
                "{} could not be taken from everything else, so what is typed on it also reaches \
                 the console behind this program: {error}",
                path.display()
            );
        }

        let given = match device.as_fd().try_clone_to_owned() {
            Ok(given) => given,
            Err(error) => {
                warn!(
                    target: "zgui::platform",
                    "the descriptor {} was opened on cannot be copied, so libinput cannot read it: \
                     {error}",
                    path.display()
                );
                self.session.close_input(path);
                return Err(EIO);
            }
        };

        self.held.push(Held {
            given: given.as_raw_fd(),
            path: path.to_owned(),
            device,
        });
        Ok(given)
    }

    /// Takes a descriptor back, and gives the device back to the session with it.
    ///
    /// A descriptor this never handed out is closed and nothing else: the session holds no record
    /// of a device it did not open.
    fn close(&mut self, fd: OwnedFd) {
        let given = fd.as_raw_fd();
        // Closed here, so that the node is free before the session is told about it. A daemon that
        // is asked to take a device back while this process still holds a descriptor onto it is
        // being told something that is not yet true.
        drop(fd);

        let Some(at) = self.held.iter().position(|held| held.given == given) else {
            return;
        };
        let held = self.held.remove(at);
        // The grab goes with it.
        drop(held.device);
        self.session.close_input(&held.path);
    }
}

/// Linux's `ENODEV`, the answer a revoked node gives.
const ENODEV: i32 = 19;

/// Linux's `EACCES`, for a node this run may not have.
const EACCES: i32 = 13;

/// Linux's `EIO`, for an open that went wrong after the node was taken.
const EIO: i32 = 5;

#[cfg(test)]
mod tests {
    //! What the session is asked, and what comes back.
    //!
    //! # No real device
    //!
    //! A grab lasts as long as the descriptor does and takes the device from everything else, so a
    //! test that opened a node of this machine would take the keyboard away from whoever is running
    //! it. `Seat::open_in` takes a directory for the same reason. So what is covered here is every
    //! path that refuses, and the descriptor bookkeeping. The path that opens a device and grabs it
    //! is covered by the run on a terminal.

    use super::*;
    use zgui_libinput::Files;

    /// The flags libinput asks with, which this implementation reads for nothing: the access mode
    /// is `zgui_evdev::Device::open`'s decision and the rest comes with it.
    const ASKED_WITH: i32 = 0x8_0802;

    #[test]
    fn a_node_the_machine_does_not_have_is_refused_rather_than_held() {
        let mut session = Session::direct();
        let mut held = Vec::new();
        let mut lent = Lent::new(&mut session, &mut held);

        let refused = lent
            .open(Path::new("/dev/input/event9999"), ASKED_WITH)
            .expect_err("a node nothing has cannot be opened");

        assert_eq!(refused, EACCES, "libinput is given a number for its log");
        assert!(held.is_empty(), "and nothing is held for it");
    }

    #[test]
    fn a_path_that_is_not_an_input_device_is_refused_rather_than_held() {
        // Every machine has `/dev/null`. It opens and it is not an evdev node, so this is the
        // refusal that happens after the open rather than at it.
        let mut session = Session::direct();
        let mut held = Vec::new();
        let mut lent = Lent::new(&mut session, &mut held);

        let refused = lent
            .open(Path::new("/dev/null"), ASKED_WITH)
            .expect_err("`/dev/null` is not an input device");

        assert_eq!(refused, EACCES);
        assert!(held.is_empty());
    }

    #[test]
    fn a_refusal_is_never_zero_or_positive() {
        // libinput reads the answer as a descriptor when it is not negative, so a refusal that came
        // back as zero would be taken for the descriptor numbered zero — which is standard input.
        for refusal in [ENODEV, EACCES, EIO] {
            assert!(refusal > 0, "the trait takes a positive number: {refusal}");
        }
    }

    #[test]
    fn a_descriptor_this_never_handed_out_is_closed_and_nothing_else() {
        // libinput closes what it was given. A descriptor from anywhere else names no device the
        // session opened, so telling the session about it would take back somebody else's.
        let mut session = Session::direct();
        let mut held = Vec::new();
        let mut lent = Lent::new(&mut session, &mut held);

        let stranger = std::fs::File::open("/dev/null").expect("every machine has `/dev/null`");
        lent.close(OwnedFd::from(stranger));

        assert!(held.is_empty());
        assert!(
            session.asked().is_empty(),
            "the session was told nothing about a device it never opened"
        );
    }
}
