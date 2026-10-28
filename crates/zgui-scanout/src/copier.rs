//! The one thing this crate does, stated so that a second device API can do it too.

use crate::buffer::Rect;
use crate::error::Error;

/// A device that can copy rectangles between the buffers it has been given.
///
/// A trait rather than one type because the device API is the part that varies. The GL
/// implementation is [`egl`](crate::egl); a Vulkan one would import the same descriptors through
/// `VK_EXT_external_memory_dma_buf` and copy with `vkCmdCopyImage`.
///
/// # Why it is two halves
///
/// Because most of a copy is not the caller's to spend. Measured on the hardware this was written
/// for: 1.07 ms of wall clock against 0.29 ms of processor, so about three quarters of it is the
/// device working while the processor has nothing to do. [`Copier::begin`] issues the work and
/// answers; [`Copier::finish`] waits for it. Whatever the caller does in between is free.
///
/// Both halves stay on one thread, and deliberately. A thread of its own was tried and was *worse*
/// — the machine has one core, and the driver the caller is composing with holds it — so the
/// overlap has to come from the device rather than from a second thread. See [`Copier::finish`].
pub trait Copier {
    /// Starts copying each of `rects` from the buffer at `from` to the buffer at `to`.
    ///
    /// The rectangles are in buffer coordinates and mean the same region in both, because that is
    /// what a repair is: the same pixels, at the same place, in a buffer that is behind. An empty
    /// rectangle is skipped, and an empty list is a copy of nothing that still answers.
    ///
    /// **Nothing waits here**, and nothing may read or write the buffer at `to` until
    /// [`Copier::finish`] has answered. A copy already running is waited for first, so a caller that
    /// misses a `finish` loses the overlap rather than the ordering.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Driver`] where the device refused, and [`Error::NoCopy`] where it offers no
    /// way to copy at all. A slot number outside the set it was given is [`Error::Driver`] as well,
    /// because it is the caller's arithmetic that was wrong and the message has to say so.
    fn begin(&mut self, from: usize, to: usize, rects: &[Rect]) -> Result<(), Error>;

    /// Waits for the copy [`Copier::begin`] started.
    ///
    /// Answers at once where nothing was started, because nothing started is nothing to wait for.
    ///
    /// **A device that exports a fence descriptor could avoid this wait entirely** by handing the
    /// descriptor to whatever the caller commits next — a display that takes an `IN_FENCE_FD` would
    /// then do the waiting in the kernel. The hardware this was written against exports none, so
    /// this waits instead.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Driver`] where the wait itself failed. A copy that was refused reported
    /// that from [`Copier::begin`], so there is nothing left here to refuse.
    fn finish(&mut self) -> Result<(), Error>;

    /// Starts every one of `steps`, in order, as one piece of work.
    ///
    /// `(from, to, rects)` each, and they are ordered: a later step's pixels land on top of an
    /// earlier step's. **This is not the same as calling [`Copier::begin`] several times** — that
    /// waits for each before recording the next, and the wait is the caller's own thread. Building
    /// a picture out of two copies is exactly the case that needs them issued together.
    ///
    /// The default asks for them one at a time, which is correct and slower. An implementation that
    /// can record several under one take of the device should say so by overriding this.
    ///
    /// # Errors
    ///
    /// Whatever [`Copier::begin`] returns, for the first step that is refused. Steps before it were
    /// issued and steps after it were not.
    fn begin_all(&mut self, steps: &[(usize, usize, &[Rect])]) -> Result<(), Error> {
        for (from, to, rects) in steps {
            self.begin(*from, *to, rects)?;
        }
        Ok(())
    }

    /// Starts a copy and waits for it, for a caller with nothing to do in between.
    ///
    /// # Errors
    ///
    /// Whatever [`Copier::begin`] and [`Copier::finish`] return.
    fn copy(&mut self, from: usize, to: usize, rects: &[Rect]) -> Result<(), Error> {
        self.begin(from, to, rects)?;
        self.finish()
    }

    /// How many buffers this copier holds.
    fn len(&self) -> usize;

    /// Returns `true` where it holds none.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
