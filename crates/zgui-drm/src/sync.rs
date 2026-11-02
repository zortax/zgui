//! A buffer's outstanding writes, as a descriptor the kernel can wait on.
//!
//! A display engine reads a buffer at the vertical blank after the commit that named it, and the
//! frame a graphics device draws into that buffer can still be unfinished then. Something has to
//! wait for it. The kernel does the wait when a plane is given an `IN_FENCE_FD`, and this is one of
//! the two ways to get one.
//!
//! The other way asks the **graphics driver** — a Vulkan semaphore exported as a sync file, or
//! `EGL_ANDROID_native_fence_sync`. This asks the **buffer**: a dma-buf carries the fences of
//! everything writing it, whichever driver that is, and hands them over as one sync file. So it
//! works where the graphics driver exports nothing, and it covers a buffer more than one device
//! writes.
//!
//! # What has to have happened first
//!
//! The writing has to have been **submitted**. A fence reaches the buffer when the command stream
//! carrying it reaches the kernel, so a caller flushes its graphics API and then asks. The work
//! need not have run: a sync file describes work that is on its way, and handing that to the kernel
//! is the point.
//!
//! A buffer nothing is writing answers a descriptor that is already signalled, rather than nothing.

use std::os::fd::{BorrowedFd, FromRawFd, OwnedFd};

use crate::error::{Error, Result};
use crate::ioctl;

/// The fence covers everything **writing** the buffer, which is what a reader waits for.
///
/// The other bit, `DMA_BUF_SYNC_WRITE`, asks for the readers too, and a display engine is a reader
/// itself — a frame that waited for the last flip to be read would wait a whole refresh interval
/// for nothing.
const READ: u32 = 1 << 0;

/// What the kernel is asked, and the descriptor it answers with.
///
/// `struct dma_buf_export_sync_file` from `linux/dma-buf.h`. It is written here rather than
/// generated from the header, which is how every other structure this crate sends the kernel is
/// made. Two reasons: the header is GPL-2.0 with the syscall note, where the DRM headers this crate
/// vendors are MIT, so copying its text into this tree is a licensing decision rather than a
/// mechanical one; and this structure is two fixed-width scalars with no union, no pointer and no
/// alignment that changes with the word size, which is the case the generator protects against.
///
/// The size is what the request number is computed from, so it is asserted below and the number is
/// asserted against the header's own arithmetic in [`crate::ioctl`].
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ExportSyncFile {
    /// Which accesses the answer has to cover. [`READ`] here and nothing else.
    flags: u32,
    /// Where the kernel writes the descriptor.
    fd: i32,
}

/// Returns a descriptor that signals when everything now writing `buffer` has finished.
///
/// `buffer` is a dma-buf descriptor — what `gbm_bo_get_fd` and `DRM_IOCTL_PRIME_HANDLE_TO_FD`
/// answer. Whatever this returns may be handed to a commit as a plane's `IN_FENCE_FD`.
///
/// Answers `Ok(None)` where the kernel does not serve this request, which is a kernel older than
/// 5.20 or a descriptor that is not a dma-buf. The caller then waits for the graphics device
/// itself, the way it did before.
///
/// # Errors
///
/// Returns [`Error::Ioctl`] where the kernel refused for any other reason.
pub fn writers_of(buffer: BorrowedFd<'_>) -> Result<Option<OwnedFd>> {
    let mut request = ExportSyncFile {
        flags: READ,
        fd: -1,
    };
    match ioctl::issue(buffer, ioctl::DMA_BUF_EXPORT_SYNC_FILE, &mut request) {
        Ok(()) => {}
        // What a kernel answers for a request the descriptor does not serve, which is a kernel
        // older than 5.20 and anything that is not a dma-buf.
        Err(Error::Ioctl { source, .. })
            if source.raw_os_error() == Some(rustix::io::Errno::NOTTY.raw_os_error()) =>
        {
            return Ok(None);
        }
        Err(refusal) => return Err(refusal),
    }
    if request.fd < 0 {
        return Ok(None);
    }
    // SAFETY: the kernel reported success and wrote a descriptor it opened for this call, holding
    // no copy of it, so this is the only owner. The one sentinel it may write is a negative number,
    // which is handled immediately above.
    Ok(Some(unsafe { OwnedFd::from_raw_fd(request.fd) }))
}

#[cfg(test)]
mod tests {
    //! The structure's size, which the request number is computed from, and what a descriptor that
    //! is not a dma-buf answers.

    use super::*;
    use std::os::fd::AsFd as _;

    #[test]
    fn the_structure_is_the_size_the_header_says() {
        // `struct dma_buf_export_sync_file { __u32 flags; __s32 fd; }`. A structure of the wrong
        // size produces a different request number, and the kernel refuses that with `EINVAL` and
        // no further explanation — so this is where it shows up instead.
        assert_eq!(size_of::<ExportSyncFile>(), 8);
        assert_eq!(align_of::<ExportSyncFile>(), 4);
    }

    #[test]
    fn a_descriptor_that_is_not_a_buffer_answers_nothing_rather_than_failing() {
        // Every machine has this one, hardware or not: an eventfd is a descriptor the kernel will
        // not serve this request on, and the answer has to be the same "this machine cannot" that
        // an old kernel gives, because a caller acts on both the same way.
        let channel = rustix::event::eventfd(0, rustix::event::EventfdFlags::CLOEXEC)
            .expect("an eventfd can be made");
        let asked = writers_of(channel.as_fd());
        assert!(
            matches!(asked, Ok(None)),
            "an eventfd answered {asked:?} rather than reporting that it carries no fences",
        );
    }
}
