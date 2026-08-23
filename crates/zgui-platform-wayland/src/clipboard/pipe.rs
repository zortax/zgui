//! Moving a selection's bytes through a pipe without stopping the loop.
//!
//! A selection belongs to another process. Reading it means asking that process to write into a
//! pipe and then reading the pipe, and there is no bound on how long it takes to answer — or
//! whether it answers at all. Writing one is the same in reverse.
//!
//! So every transfer here waits on the file descriptor rather than on the read, with a deadline,
//! and the deadline is what separates "slow" from "never". Neither direction ever runs on the
//! thread that reads input for more than the short bound below.

use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags};

/// How long a transfer started for an answer somebody is waiting on may take.
///
/// Long enough for another process to be scheduled and answer; short enough that a selection owner
/// that has hung is a failed paste rather than a hung application.
pub const PATIENCE: Duration = Duration::from_secs(4);

/// How long a transfer may take when the thread waiting is the one that also reads input.
///
/// A quarter of a second is what this project already calls a human-visible wait. Past it, the
/// answer is that the platform could not answer at once, and the caller asks again through the
/// loop — where there is no bound on the thread at all.
pub const IMPATIENCE: Duration = Duration::from_millis(250);

/// The largest selection this backend will take.
///
/// A selection is text, and text a person copied. The bound is here because the other end of the
/// pipe is another process, which is free to write for ever, and a paste that filled memory would
/// take the application with it.
pub const LIMIT: usize = 32 * 1024 * 1024;

/// Reads everything the other end writes, up to a deadline.
///
/// Answers with what arrived before the deadline, or why it did not.
pub fn read(source: impl Read + AsFd, patience: Duration) -> Result<Vec<u8>, Failed> {
    let deadline = Instant::now() + patience;
    let mut source = source;
    let mut taken = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        wait(source.as_fd(), PollFlags::IN, deadline)?;
        // A read of zero is the other end closing, which is how a transfer ends: there is no
        // length in front of the bytes.
        match source.read(&mut chunk) {
            Ok(0) => return Ok(taken),
            Ok(read) => {
                if taken.len() + read > LIMIT {
                    return Err(Failed::TooLarge);
                }
                taken.extend_from_slice(&chunk[..read]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(Failed::Broken(error.to_string())),
        }
    }
}

/// Writes everything, up to a deadline.
///
/// A destination that stops reading — because it was killed, or lost interest — leaves this
/// waiting, which is why it has a deadline of its own. A partial write is not reported as an
/// error to anybody: the destination sees a short selection, which is what actually happened.
pub fn write(destination: impl Write + AsFd, bytes: &[u8], patience: Duration) {
    let deadline = Instant::now() + patience;
    let mut destination = destination;
    let mut left = bytes;
    while !left.is_empty() {
        if wait(destination.as_fd(), PollFlags::OUT, deadline).is_err() {
            return;
        }
        match destination.write(left) {
            Ok(0) => return,
            Ok(written) => left = &left[written..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return,
        }
    }
}

/// Waits for `descriptor` to be ready, or for the deadline to pass.
fn wait(descriptor: BorrowedFd<'_>, flags: PollFlags, deadline: Instant) -> Result<(), Failed> {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Failed::TimedOut);
        }
        let mut watched = [PollFd::new(&descriptor, flags)];
        match rustix::event::poll(&mut watched, Some(&left.try_into().unwrap_or_default())) {
            Ok(0) => return Err(Failed::TimedOut),
            Ok(_) => return Ok(()),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(Failed::Broken(error.to_string())),
        }
    }
}

/// Why a transfer did not finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failed {
    /// The other end did not answer in time.
    TimedOut,
    /// The other end wrote more than this backend will take.
    TooLarge,
    /// The descriptor failed, in the system's own words.
    Broken(String),
}

impl From<Failed> for zgui_platform::ClipboardError {
    fn from(failed: Failed) -> Self {
        match failed {
            Failed::TimedOut => Self::TimedOut,
            Failed::TooLarge => Self::Backend("the selection was too large to take".to_owned()),
            Failed::Broken(reason) => Self::Backend(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Failed, IMPATIENCE, LIMIT, PATIENCE, read, write};
    use std::time::Duration;

    fn pipe() -> (std::fs::File, std::fs::File) {
        let (reader, writer) = rustix::pipe::pipe().expect("a pipe can be made");
        (reader.into(), writer.into())
    }

    #[test]
    fn a_transfer_ends_when_the_other_end_closes() {
        // There is no length in front of the bytes: the close *is* the end of the selection.
        let (reader, mut writer) = pipe();
        std::io::Write::write_all(&mut writer, b"hello").expect("the pipe took the bytes");
        drop(writer);
        assert_eq!(read(reader, PATIENCE), Ok(b"hello".to_vec()));
    }

    #[test]
    fn an_owner_that_never_answers_times_out_rather_than_waiting_for_ever() {
        // The writer is held open and writes nothing, which is exactly what a hung selection
        // owner looks like from here.
        let (reader, writer) = pipe();
        assert_eq!(
            read(reader, Duration::from_millis(60)),
            Err(Failed::TimedOut)
        );
        drop(writer);
    }

    #[test]
    fn an_empty_selection_is_an_answer_rather_than_a_failure() {
        let (reader, writer) = pipe();
        drop(writer);
        assert_eq!(read(reader, PATIENCE), Ok(Vec::new()));
    }

    #[test]
    fn a_transfer_arriving_in_pieces_is_reassembled() {
        let (reader, mut writer) = pipe();
        std::thread::spawn(move || {
            for piece in [b"one ".as_slice(), b"two ", b"three"] {
                std::io::Write::write_all(&mut writer, piece).expect("the pipe took the bytes");
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        assert_eq!(read(reader, PATIENCE), Ok(b"one two three".to_vec()));
    }

    #[test]
    fn what_is_written_is_what_is_read_back() {
        let (reader, writer) = pipe();
        let sent = "a selection somebody copied".repeat(400);
        let bytes = sent.clone().into_bytes();
        std::thread::spawn(move || write(writer, &bytes, PATIENCE));
        assert_eq!(read(reader, PATIENCE), Ok(sent.into_bytes()));
    }

    #[test]
    fn the_impatient_bound_is_shorter_than_the_patient_one() {
        // One is for a thread nobody is waiting on; the other is for the thread that reads input.
        const { assert!(IMPATIENCE.as_millis() < PATIENCE.as_millis()) };
        const { assert!(LIMIT > 0) };
    }
}
