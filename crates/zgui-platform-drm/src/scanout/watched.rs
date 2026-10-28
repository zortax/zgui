//! A frame the card is still drawing, as a descriptor the loop can park on.
//!
//! A driver that exports no sync file leaves this program to wait for its own drawing, and that
//! wait is the whole time the card takes. A loop that waits for it answers no key, fires no timer
//! and reads no page flip while it does, which is most of a frame.
//!
//! # A thread is not the answer, measured
//!
//! Waiting on `EGL_KHR_fence_sync` from a second thread looks like it moves the wait off the loop
//! and does not: `eglClientWaitSyncKHR` holds the graphics driver's own lock for as long as it
//! waits, so the loop's next call into that driver blocks behind it for the rest of the wait.
//! Measured on the machine this was written for, the loop sat in `futex` for 13.42 ms while the
//! thread sat in `DRM_IOCTL_NOUVEAU_GEM_CPU_PREP` for 13.46. The wait had moved and nothing had
//! been gained.
//!
//! # What this is instead
//!
//! The buffer answers a **sync file** — [`zgui_drm::sync::writers_of`] — and a sync file is an
//! ordinary descriptor that becomes readable when the fence signals. So the frame goes into the
//! loop's own poll set beside the card, the wake channel and the input devices, and the loop learns
//! that the card has finished in the one place it is allowed to block. Nothing here calls into the
//! graphics driver at all, so nothing here can hold its lock.

use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rustix::event::{PollFd, PollFlags, Timespec, poll};

/// Returning at once rather than waiting, for the question "has it finished yet".
const AT_ONCE: Timespec = Timespec {
    tv_sec: 0,
    tv_nsec: 0,
};

/// A frame the card was given, and the descriptor that says when it has drawn it.
///
/// One per frame, because a display may have more than one frame on the card at a time: the
/// descriptor travels with the frame rather than with the display, so a frame that finishes tells
/// no other frame's story.
#[derive(Debug)]
pub(crate) struct Watched(OwnedFd);

impl Watched {
    /// Takes `fence` as the descriptor a frame has finished on.
    pub(crate) fn on(fence: OwnedFd) -> Self {
        Self(fence)
    }

    /// Whether the card has finished drawing this frame.
    ///
    /// A poll of no length, which is a system call and nothing more. It reaches no graphics driver
    /// and takes no lock, so a caller may ask as often as it likes.
    ///
    /// Answers `true` where the kernel refuses the question. A frame nothing can report on would
    /// otherwise be held forever, and showing it a refresh early is the lesser fault.
    pub(crate) fn drawn(&self) -> bool {
        let mut asked = [PollFd::from_borrowed_fd(self.0.as_fd(), PollFlags::IN)];
        match poll(&mut asked, Some(&AT_ONCE)) {
            Ok(0) => false,
            Ok(_) => true,
            Err(_) => true,
        }
    }

    /// The descriptor, for the loop to park on until the card has finished.
    pub(crate) fn descriptor(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
