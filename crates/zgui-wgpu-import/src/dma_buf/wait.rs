//! Waiting for a decoder's writes to a dma-buf to finish.
//!
//! A dma-buf descriptor polls readable once every write fence the kernel tracks for it has
//! signalled. A decoder's writes are such fences, so this is the moment the samples are complete.

use rustix::event::{PollFd, PollFlags, Timespec, poll};

use super::DmaBufPlane;
use crate::ImportError;

/// How long a decoder may take to finish one frame before the import gives up.
const PATIENCE: Timespec = Timespec {
    tv_sec: 1,
    tv_nsec: 0,
};

/// Waits until every write to `planes` has finished.
pub(super) fn written(planes: &[DmaBufPlane<'_>]) -> Result<(), ImportError> {
    let mut fds: Vec<PollFd<'_>> = planes
        .iter()
        .map(|plane| PollFd::new(&plane.fd, PollFlags::IN))
        .collect();
    loop {
        match poll(&mut fds, Some(&PATIENCE)) {
            Ok(_)
                if fds.iter().any(|fd| {
                    fd.revents()
                        .intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL)
                }) =>
            {
                return Err(ImportError::Platform(
                    "a dma-buf descriptor reported an error while waiting for its writes"
                        .to_owned(),
                ));
            }
            Ok(_) if fds.iter().all(|fd| fd.revents().contains(PollFlags::IN)) => return Ok(()),
            Ok(0) => {
                return Err(ImportError::Platform(
                    "the decoder did not finish writing the frame within a second".to_owned(),
                ));
            }
            // Some descriptors are ready: wait again on the rest.
            Ok(_) => fds.retain(|fd| !fd.revents().contains(PollFlags::IN)),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(ImportError::Platform(format!("poll: {error}"))),
        }
    }
}
