//! What the device says back: a flip completed, and when.
//!
//! These events pace a frame loop that has no compositor under it. A flip is asked for and returns
//! at once; the buffer it named stays busy until an event says otherwise, and the moment that
//! event carries is the vertical blank the frame reached.

use std::time::Duration;

use crate::device::Device;
use crate::error::{Error, Result};
use crate::sys;

/// Something the device reported.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A page flip finished, and its buffer is free.
    FlipComplete {
        /// Which CRTC finished.
        crtc: u32,
        /// When the vertical blank happened, on the same monotonic clock the rest of the loop
        /// reads.
        at: Duration,
        /// What was passed as this flip's user data.
        user_data: u64,
    },
}

/// How many bytes are read from the device at once.
///
/// A flip event is 32 bytes. Reading a page's worth means a burst of completions across several
/// CRTCs arrives in one call rather than one per call.
const BUFFER: usize = 4096;

impl Device {
    /// Reads whatever the device has to say, without waiting.
    ///
    /// Every descriptor a [`Device`] holds is non-blocking, so this returns an empty vector when
    /// nothing has happened yet. A frame loop parks on the descriptor and calls this when it
    /// wakes.
    ///
    /// A caller matches a completion against its own flip by the CRTC it flipped. Nothing in this
    /// crate sets the user data — both commit paths pass zero — so a completion carries no token of
    /// the caller's own to match on:
    ///
    /// ```no_run
    /// use zgui_drm::{Device, Event};
    ///
    /// let device = Device::open_first()?;
    /// let mine = device.resources()?.crtcs[0];
    ///
    /// let flipped = device.poll_events()?.into_iter().any(|event| {
    ///     matches!(event, Event::FlipComplete { crtc, .. } if crtc == mine)
    /// });
    /// # Ok::<(), zgui_drm::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unusable`] when the read fails for a reason other than there being nothing
    /// to read.
    pub fn poll_events(&self) -> Result<Vec<Event>> {
        let mut bytes = [0_u8; BUFFER];
        let read = match rustix::io::read(self.fd(), &mut bytes[..]) {
            Ok(read) => read,
            // `EAGAIN` is the device saying the queue is empty, which is the ordinary answer on a
            // non-blocking descriptor. `EINTR` is a signal that arrived first. Either way nothing
            // was reported, and the caller asks again when it next wakes.
            Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => return Ok(Vec::new()),
            Err(errno) => {
                return Err(Error::Unusable(format!(
                    "cannot read events from {}: {errno}",
                    self.path().display()
                )));
            }
        };
        Ok(parse(&bytes[..read]))
    }
}

/// Reads the events out of one read's worth of bytes.
///
/// The kernel writes whole records, so a well-formed stream ends on a record boundary. This walk
/// checks every length anyway: without those checks a malformed stream is an endless loop or a
/// read past the end.
fn parse(bytes: &[u8]) -> Vec<Event> {
    let mut events = Vec::new();
    let mut rest = bytes;

    while rest.len() >= size_of::<sys::drm_event>() {
        // SAFETY: the loop runs only while `rest` holds at least a header, so the read stays
        // inside it. The read is unaligned because `rest` is a slice of `u8` and carries no
        // stronger alignment. `drm_event` is two `u32`, so every bit pattern of those bytes is a
        // value of it.
        let header: sys::drm_event = unsafe { std::ptr::read_unaligned(rest.as_ptr().cast()) };
        let length = header.length as usize;

        // A record shorter than its own header never advances the walk, and one that runs past
        // what was read has bytes missing. Either means the stream is something other than what
        // this crate reads, so the walk stops here.
        if length < size_of::<sys::drm_event>() || length > rest.len() {
            break;
        }

        if header.type_ == sys::DRM_EVENT_FLIP_COMPLETE
            && length >= size_of::<sys::drm_event_vblank>()
        {
            // SAFETY: the same claims as the header read, and `length` is both within `rest` and
            // at least the size of a `drm_event_vblank`. Every field of that structure is an
            // unsigned integer, so every bit pattern of those bytes is a value of it.
            let vblank: sys::drm_event_vblank =
                unsafe { std::ptr::read_unaligned(rest.as_ptr().cast()) };
            events.push(Event::FlipComplete {
                crtc: vblank.crtc_id,
                // The kernel splits one monotonic instant into whole seconds and the microseconds
                // under them, so `tv_usec` is at most 999_999. The two parts are added instead of
                // multiplied out because this function has to survive a stream it does not
                // recognise, and a multiplication is the one operation in it that a debug build
                // could panic on. The two spellings agree for every value the kernel produces.
                at: Duration::from_secs(u64::from(vblank.tv_sec))
                    + Duration::from_micros(u64::from(vblank.tv_usec)),
                user_data: vblank.user_data,
            });
        }

        rest = &rest[length..];
    }

    events
}

#[cfg(test)]
mod tests {
    //! The walk over a read's worth of bytes, including bytes the kernel would never write.
    //!
    //! The records are built here, so a length the kernel cannot produce is as easy to feed in as
    //! a real flip. No hardware test reaches the checks that keep a bad stream from looping or
    //! from reading past its end.

    use super::*;

    /// Returns the bytes of one flip event, as the kernel lays it out.
    fn flip(crtc: u32, tv_sec: u32, tv_usec: u32, user_data: u64) -> Vec<u8> {
        record(
            sys::DRM_EVENT_FLIP_COMPLETE,
            size_of::<sys::drm_event_vblank>() as u32,
            crtc,
            tv_sec,
            tv_usec,
            user_data,
        )
    }

    /// Returns the bytes of one 32-byte record, with the header written as told.
    ///
    /// `kind` and `length` are separate from the payload so a test can state a length the payload
    /// does not have.
    fn record(
        kind: u32,
        length: u32,
        crtc: u32,
        tv_sec: u32,
        tv_usec: u32,
        user_data: u64,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&kind.to_ne_bytes());
        bytes.extend_from_slice(&length.to_ne_bytes());
        bytes.extend_from_slice(&user_data.to_ne_bytes());
        bytes.extend_from_slice(&tv_sec.to_ne_bytes());
        bytes.extend_from_slice(&tv_usec.to_ne_bytes());
        // The sequence number, which this crate reads nothing out of.
        bytes.extend_from_slice(&0_u32.to_ne_bytes());
        bytes.extend_from_slice(&crtc.to_ne_bytes());
        bytes
    }

    #[test]
    fn a_flip_event_carries_its_crtc_its_moment_and_its_user_data() {
        let bytes = flip(42, 7, 250_000, 0xdead_beef);

        assert_eq!(
            parse(&bytes),
            [Event::FlipComplete {
                crtc: 42,
                at: Duration::new(7, 250_000_000),
                user_data: 0xdead_beef,
            }],
            "the fields come out of the record the kernel wrote them into"
        );
    }

    #[test]
    fn two_events_in_one_read_both_come_out() {
        let mut bytes = flip(1, 1, 0, 10);
        bytes.extend(flip(2, 2, 0, 20));

        assert_eq!(
            parse(&bytes),
            [
                Event::FlipComplete {
                    crtc: 1,
                    at: Duration::new(1, 0),
                    user_data: 10,
                },
                Event::FlipComplete {
                    crtc: 2,
                    at: Duration::new(2, 0),
                    user_data: 20,
                },
            ],
            "one read holds as many completions as fit in it"
        );
    }

    #[test]
    fn a_record_that_says_it_is_empty_stops_the_walk() {
        // A zero length advances nothing, so a walk that trusted it would read this record for
        // ever. This test hangs rather than fails if that check goes away.
        let mut bytes = record(sys::DRM_EVENT_FLIP_COMPLETE, 0, 1, 1, 0, 10);
        bytes.extend(flip(2, 2, 0, 20));

        assert!(
            parse(&bytes).is_empty(),
            "a length that cannot be walked past ends the read, and what follows is not trusted"
        );
    }

    #[test]
    fn a_record_longer_than_what_was_read_stops_the_walk() {
        let bytes = record(
            sys::DRM_EVENT_FLIP_COMPLETE,
            size_of::<sys::drm_event_vblank>() as u32 + 8,
            1,
            1,
            0,
            10,
        );

        assert!(
            parse(&bytes).is_empty(),
            "a record with bytes missing is not reported as if it were whole"
        );
    }

    #[test]
    fn an_unknown_record_is_stepped_over_and_the_next_flip_still_parses() {
        // The kernel sends vblank and sequence events down the same descriptor, and a driver may
        // send its own. A poll has to reach the flip behind such a record.
        let mut bytes = record(
            0x8000_0001,
            size_of::<sys::drm_event_vblank>() as u32,
            9,
            9,
            0,
            99,
        );
        bytes.extend(flip(3, 5, 500_000, 30));

        assert_eq!(
            parse(&bytes),
            [Event::FlipComplete {
                crtc: 3,
                at: Duration::new(5, 500_000_000),
                user_data: 30,
            }],
            "a record of a type this crate does not model costs its own bytes and nothing else"
        );
    }

    #[test]
    fn a_partial_header_at_the_end_is_left_alone() {
        let mut bytes = flip(1, 1, 0, 10);
        bytes.extend_from_slice(&[0_u8; 4]);

        assert_eq!(
            parse(&bytes).len(),
            1,
            "fewer bytes than a header is the end of the read"
        );
    }
}
