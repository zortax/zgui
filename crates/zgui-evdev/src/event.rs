//! Reading what a device reports, in the batches the kernel groups it into.
//!
//! The kernel writes fixed-size records and ends each coherent update with a `SYN_REPORT`. The
//! grouping is part of the interface: a pointer moved diagonally sends `REL_X`, `REL_Y` and
//! `SYN_REPORT`, and a reader that took those for two events would move the pointer twice, along
//! one axis each time. So a read hands back batches, and a batch is one update.
//!
//! # The reader takes a descriptor
//!
//! Reading an `/dev/input/eventN` node is `read()` and nothing else, so [`Reader`] takes a
//! descriptor. The whole of the batching is then exercised over a pipe with no hardware present,
//! including the cases a real device never produces: a read that ends in the
//! middle of a record, an update that spans two reads, and a stream that stops before its report
//! arrives.

use std::time::Duration;

use rustix::fd::BorrowedFd;

use crate::code::{Absolute, EventType, Key, Relative, Synchronisation};
use crate::error::{Error, Result};
use crate::sys;

/// The size of one record, as the kernel lays it out.
const RECORD: usize = size_of::<sys::input_event>();

/// How many records are read at once.
///
/// A read that comes back full means more is queued, so this is the step the drain below takes
/// rather than a limit on what one call returns.
const RECORDS: usize = 128;

/// One record the kernel wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    /// When the kernel timestamped it.
    ///
    /// [`Device::open`](crate::Device::open) asks for the monotonic clock, so this is normally
    /// time since the machine started and it only moves forward. A kernel or a driver may refuse
    /// that, leaving the stream on `CLOCK_REALTIME` — time since the epoch, which a clock step
    /// moves — and [`Device::has_monotonic_timestamps`](crate::Device::has_monotonic_timestamps)
    /// is what says which one a device is on.
    ///
    /// Stamping the moment a loop wakes is not a substitute. A wake carries every event queued
    /// since the last one, and one moment for all of them is what a double click and a key repeat
    /// are measured against.
    pub at: Duration,
    /// Which vocabulary [`Event::code`] is drawn from.
    pub kind: EventType,
    /// The code, which means nothing without [`Event::kind`].
    pub code: u16,
    /// What happened: `1` or `0` for a key, a distance for a relative axis, a position for an
    /// absolute one.
    pub value: i32,
}

impl Event {
    /// Returns the key this event carries, or `None` where it is not a key event.
    ///
    /// ```
    /// use std::time::Duration;
    /// use zgui_evdev::{Event, EventType, Key};
    ///
    /// let press = Event {
    ///     at: Duration::from_secs(7),
    ///     kind: EventType::EV_KEY,
    ///     code: Key::KEY_A.raw(),
    ///     value: 1,
    /// };
    ///
    /// assert_eq!(press.key(), Some(Key::KEY_A));
    /// assert_eq!(press.relative(), None, "the code is read against the type it arrived under");
    /// ```
    pub fn key(&self) -> Option<Key> {
        (self.kind == EventType::EV_KEY).then(|| Key::new(self.code))
    }

    /// Returns the relative axis this event carries, or `None` where it is not a relative event.
    pub fn relative(&self) -> Option<Relative> {
        (self.kind == EventType::EV_REL).then(|| Relative::new(self.code))
    }

    /// Returns the absolute axis this event carries, or `None` where it is not an absolute event.
    pub fn absolute(&self) -> Option<Absolute> {
        (self.kind == EventType::EV_ABS).then(|| Absolute::new(self.code))
    }

    /// Returns `true` for the `SYN_REPORT` that ends a batch.
    fn is_report(&self) -> bool {
        self.kind == EventType::EV_SYN && self.code == Synchronisation::SYN_REPORT.raw()
    }

    /// Returns the event one record holds.
    fn decode(record: &[u8; RECORD]) -> Self {
        // SAFETY: `record` is exactly the size of the structure, so the read stays inside it. The
        // read is unaligned because the bytes came out of a byte buffer and carry no stronger
        // alignment. Every field of `input_event` is an integer, so every bit pattern of those
        // bytes is a value of it.
        let raw: sys::input_event = unsafe { std::ptr::read_unaligned(record.as_ptr().cast()) };
        Self {
            // The kernel splits one instant into whole seconds and the microseconds under them.
            // Both are added rather than multiplied, and added saturating: this walk exists to
            // survive a stream it does not recognise, and arithmetic is the one thing in it that
            // a debug build could panic on. The two spellings agree for every value a kernel can
            // produce.
            at: Duration::from_secs(u64::try_from(raw.time.tv_sec).unwrap_or(0)).saturating_add(
                Duration::from_micros(u64::try_from(raw.time.tv_usec).unwrap_or(0)),
            ),
            kind: EventType::new(raw.type_),
            code: raw.code,
            value: raw.value,
        }
    }
}

/// One coherent update: everything the device reported at the same moment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Batch {
    /// The moment the terminating `SYN_REPORT` carried.
    pub at: Duration,
    /// The events, in the order the kernel wrote them.
    ///
    /// The `SYN_REPORT` that ended the batch is left out, because it is the boundary rather than
    /// something that happened. A `SYN_DROPPED` stays in: it says the kernel's queue overflowed
    /// and the events around it are incomplete, which is a fact a caller has to see.
    pub events: Vec<Event>,
}

/// Turns reads of a descriptor into batches.
///
/// State is carried between calls because neither boundary lines up with a read. A read can stop
/// in the middle of a record, and an update can span two of them, so what a call cannot finish is
/// held until the call that can.
#[derive(Debug, Default)]
pub struct Reader {
    /// Bytes of a record the last read stopped in the middle of.
    partial: Vec<u8>,
    /// Events since the last `SYN_REPORT`.
    pending: Vec<Event>,
}

impl Reader {
    /// Creates a reader with nothing held over.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads whatever `from` has to say, without waiting.
    ///
    /// The descriptor is expected to be non-blocking, and an empty queue is then an empty vector
    /// rather than a wait. The read is repeated while it comes back full, so a queue longer than
    /// one buffer is drained in one call instead of over as many wakes as it takes.
    ///
    /// Events with no `SYN_REPORT` behind them yet are held, so an update that spans two reads is
    /// still delivered whole.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Read`] when the read fails for a reason other than there being nothing to
    /// read, carrying the errno. A loop branches on that errno rather than on the message: see the
    /// variant for the case that decides whether this device is worth polling again.
    pub fn read(&mut self, from: BorrowedFd<'_>) -> Result<Vec<Batch>> {
        let mut bytes = [0_u8; RECORDS * RECORD];
        let mut batches = Vec::new();
        loop {
            let read = match rustix::io::read(from, &mut bytes[..]) {
                Ok(read) => read,
                // `EAGAIN` is the queue saying it is empty, which is the ordinary answer on a
                // non-blocking descriptor. `EINTR` is a signal that arrived first. Either way
                // nothing more is there now, and the caller asks again when it next wakes.
                Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => break,
                // The errno is carried rather than described. `ENODEV` — an unplugged device, or
                // one `logind` revoked on a terminal switch — is what tells a loop to drop this
                // device instead of polling a descriptor that is ready for ever.
                Err(errno) => {
                    return Err(Error::Read {
                        source: errno.into(),
                    });
                }
            };
            if read == 0 {
                break;
            }
            batches.append(&mut self.feed(&bytes[..read]));
            if read < bytes.len() {
                break;
            }
        }
        Ok(batches)
    }

    /// Adds one read's worth of bytes and reports the batches they completed.
    fn feed(&mut self, bytes: &[u8]) -> Vec<Batch> {
        self.partial.extend_from_slice(bytes);

        let mut batches = Vec::new();
        let mut consumed = 0;
        while self.partial.len() - consumed >= RECORD {
            let mut record = [0_u8; RECORD];
            record.copy_from_slice(&self.partial[consumed..consumed + RECORD]);
            consumed += RECORD;

            let event = Event::decode(&record);
            if event.is_report() {
                batches.push(Batch {
                    at: event.at,
                    events: std::mem::take(&mut self.pending),
                });
            } else {
                self.pending.push(event);
            }
        }
        self.partial.drain(..consumed);

        batches
    }
}

#[cfg(test)]
mod tests {
    //! The batching, over bytes written here and over a pipe.
    //!
    //! No device is needed for any of this. A record is twenty-four bytes and a read is a read, so
    //! a pipe stands in for a device exactly — and the cases worth asserting are the ones a
    //! working device never produces: a read cut in the middle of a record, an update split across
    //! two reads, and a stream that ends before its report.

    use std::io::Write;

    use rustix::fd::AsFd;

    use super::*;

    /// The bytes of one record, as the kernel lays it out.
    fn record(at: Duration, kind: EventType, code: u16, value: i32) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(RECORD);
        let seconds = i64::try_from(at.as_secs()).expect("the test uses a small moment");
        bytes.extend_from_slice(&seconds.to_ne_bytes());
        bytes.extend_from_slice(&i64::from(at.subsec_micros()).to_ne_bytes());
        bytes.extend_from_slice(&kind.raw().to_ne_bytes());
        bytes.extend_from_slice(&code.to_ne_bytes());
        bytes.extend_from_slice(&value.to_ne_bytes());
        bytes
    }

    /// The bytes of one `SYN_REPORT`.
    fn report(at: Duration) -> Vec<u8> {
        record(at, EventType::EV_SYN, Synchronisation::SYN_REPORT.raw(), 0)
    }

    /// The bytes of a pointer that moved diagonally, and said so once.
    fn diagonal(at: Duration, x: i32, y: i32) -> Vec<u8> {
        let mut bytes = record(at, EventType::EV_REL, Relative::REL_X.raw(), x);
        bytes.extend(record(at, EventType::EV_REL, Relative::REL_Y.raw(), y));
        bytes.extend(report(at));
        bytes
    }

    #[test]
    fn one_record_comes_back_with_the_fields_the_kernel_wrote() {
        let mut reader = Reader::new();
        let mut bytes = record(
            Duration::new(7, 250_000_000),
            EventType::EV_KEY,
            Key::KEY_A.raw(),
            1,
        );
        bytes.extend(report(Duration::new(7, 250_000_000)));

        let batches = reader.feed(&bytes);

        assert_eq!(batches.len(), 1);
        assert_eq!(
            batches[0].events,
            [Event {
                at: Duration::new(7, 250_000_000),
                kind: EventType::EV_KEY,
                code: Key::KEY_A.raw(),
                value: 1,
            }],
            "the seconds and the microseconds are added into one moment"
        );
        assert_eq!(batches[0].events[0].key(), Some(Key::KEY_A));
    }

    #[test]
    fn a_diagonal_move_is_one_batch_and_not_two_motions() {
        // Batches exist for this. Reported as two events, this pointer moves along one axis and
        // then the other, and a cursor follows a staircase.
        let mut reader = Reader::new();

        let batches = reader.feed(&diagonal(Duration::from_secs(1), 4, -3));

        assert_eq!(batches.len(), 1, "one update, however many axes moved");
        assert_eq!(
            batches[0]
                .events
                .iter()
                .map(|event| (event.relative(), event.value))
                .collect::<Vec<_>>(),
            [(Some(Relative::REL_X), 4), (Some(Relative::REL_Y), -3),],
            "both axes are in the batch, in the order the kernel wrote them"
        );
    }

    #[test]
    fn the_report_that_ends_a_batch_is_the_boundary_and_not_an_event() {
        let mut reader = Reader::new();

        let batches = reader.feed(&diagonal(Duration::new(2, 500_000_000), 1, 1));

        assert_eq!(batches[0].events.len(), 2, "the report is not one of them");
        assert_eq!(
            batches[0].at,
            Duration::new(2, 500_000_000),
            "the batch carries the moment the report did"
        );
    }

    #[test]
    fn an_update_split_across_two_reads_is_still_delivered_whole() {
        let mut reader = Reader::new();
        let bytes = diagonal(Duration::from_secs(1), 4, -3);
        let (first, second) = bytes.split_at(RECORD);

        assert!(
            reader.feed(first).is_empty(),
            "an update with no report yet is not a batch"
        );
        let batches = reader.feed(second);

        assert_eq!(batches.len(), 1);
        assert_eq!(
            batches[0].events.len(),
            2,
            "the event held over is at the front of the batch that completed"
        );
    }

    #[test]
    fn a_read_that_stops_in_the_middle_of_a_record_loses_nothing() {
        // A pipe can end a read anywhere. A device does not, and that is exactly why this case
        // needs asserting here rather than against hardware.
        let mut reader = Reader::new();
        let bytes = diagonal(Duration::from_secs(1), 4, -3);
        let (first, second) = bytes.split_at(RECORD + 9);

        assert!(reader.feed(first).is_empty());
        let batches = reader.feed(second);

        assert_eq!(
            batches[0].events.len(),
            2,
            "the nine bytes held over were the front of the second record"
        );
    }

    #[test]
    fn two_updates_in_one_read_come_back_as_two_batches() {
        let mut reader = Reader::new();
        let mut bytes = diagonal(Duration::from_secs(1), 1, 0);
        bytes.extend(diagonal(Duration::from_secs(2), 0, 1));

        let batches = reader.feed(&bytes);

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].at, Duration::from_secs(1));
        assert_eq!(batches[1].at, Duration::from_secs(2));
    }

    #[test]
    fn events_with_no_report_behind_them_are_held_rather_than_guessed_at() {
        let mut reader = Reader::new();
        let bytes = record(
            Duration::from_secs(1),
            EventType::EV_KEY,
            Key::KEY_A.raw(),
            1,
        );

        assert!(
            reader.feed(&bytes).is_empty(),
            "half an update is not an update"
        );
    }

    #[test]
    fn a_dropped_marker_stays_in_the_batch_it_arrived_in() {
        // `SYN_DROPPED` says the kernel's queue overflowed, so the events around it are
        // incomplete. Swallowing it would leave a caller resyncing nothing.
        let mut reader = Reader::new();
        let mut bytes = record(
            Duration::from_secs(1),
            EventType::EV_SYN,
            Synchronisation::SYN_DROPPED.raw(),
            0,
        );
        bytes.extend(report(Duration::from_secs(1)));

        let batches = reader.feed(&bytes);

        assert_eq!(
            batches[0].events.len(),
            1,
            "the overflow marker is reported, and only the report is dropped"
        );
        assert_eq!(
            batches[0].events[0].code,
            Synchronisation::SYN_DROPPED.raw()
        );
    }

    #[test]
    fn a_pipe_stands_in_for_a_device() {
        // `read()` on an evdev node is `read()`. Everything above is the same walk this takes,
        // and this is what says the descriptor half agrees with it.
        let (reader_end, mut writer_end) = std::io::pipe().expect("a pipe is made");
        let mut bytes = diagonal(Duration::from_secs(1), 4, -3);
        bytes.extend(diagonal(Duration::from_secs(2), -1, 2));
        writer_end
            .write_all(&bytes)
            .expect("the records are written");
        // The writer is closed so the drain below ends on an empty read rather than a wait.
        drop(writer_end);

        let mut reader = Reader::new();
        let batches = reader
            .read(reader_end.as_fd())
            .expect("a pipe with records in it reads");

        assert_eq!(batches.len(), 2, "both updates arrive in one call");
        assert_eq!(batches[0].events.len(), 2);
        assert_eq!(batches[1].events.len(), 2);
    }

    #[test]
    fn a_failed_read_hands_back_the_errno_rather_than_a_sentence() {
        // A loop has to tell a device that is gone from one that had a passing failure, and the
        // only thing that says which is the errno. Reading the writing end of a pipe is a failure
        // that needs no device: the descriptor is open for writing, so the kernel answers `EBADF`.
        let (_reader_end, writer_end) = std::io::pipe().expect("a pipe is made");

        let failure = Reader::new()
            .read(writer_end.as_fd())
            .expect_err("a descriptor open for writing cannot be read");

        let Error::Read { source } = &failure else {
            panic!("a read that failed is reported as one: {failure:?}");
        };
        assert_eq!(
            source.raw_os_error(),
            Some(rustix::io::Errno::BADF.raw_os_error()),
            "the errno survives, which is what `ENODEV` has to do on a revoked device"
        );
        assert!(
            std::error::Error::source(&failure).is_some(),
            "and it is reachable through `source`, like every other failure here"
        );
    }

    #[test]
    fn a_stream_longer_than_one_buffer_is_drained_in_one_call() {
        // A read that comes back full means more is queued. Stopping there would leave a device
        // that reports faster than the caller wakes permanently behind.
        let (reader_end, mut writer_end) = std::io::pipe().expect("a pipe is made");
        // Three records to an update and a buffer of `RECORDS` records, so this is three buffers.
        let updates = RECORDS;
        let mut bytes = Vec::new();
        for step in 0..updates {
            bytes.extend(diagonal(
                Duration::from_secs(u64::try_from(step).expect("the step is small")),
                1,
                1,
            ));
        }
        writer_end
            .write_all(&bytes)
            .expect("the records are written");
        drop(writer_end);

        let mut reader = Reader::new();
        let batches = reader
            .read(reader_end.as_fd())
            .expect("a pipe with records in it reads");

        assert_eq!(
            batches.len(),
            updates,
            "three buffers' worth of records all arrive from one call"
        );
    }
}
