//! Where a frame lands, and the flip that puts it on the screen.
//!
//! A display scans out of one buffer while the next frame is written into the other, and a flip
//! swaps which is which at the vertical blank. [`Scanout`] owns that pair for one output: the two
//! buffers, the two framebuffers the kernel knows them by, which one is off screen, and whether a
//! flip is still on its way.
//!
//! The copy into the back buffer is all the CPU cost here. It is a copy rather than a swizzle
//! because the fourcc is chosen to match the order the renderer read the frame back in, which
//! [`Pixels::is_bgra`] answers.

use tracing::warn;
use zgui_drm::buffer::DumbBuffer;
use zgui_drm::commit::{Commit, Pipe};
use zgui_drm::format::Format;
use zgui_drm::framebuffer::Framebuffer;
use zgui_drm::{Device, Event};
use zgui_platform::PlatformError;
use zgui_render_wgpu::Pixels;

use crate::output::{Output, backend};

/// How many buffers a display is driven from.
///
/// Two: one on the screen while the other is written. One would tear, and a third buys latency
/// this backend has nothing to spend it on — a console draws when a frame is asked for, and the
/// flip is what paces it.
const BUFFERS: usize = 2;

/// How many bytes one pixel takes, in the readback and in the buffer alike.
///
/// [`Pixels`] states four, and every fourcc [`fourcc`] answers is a 32-bit format.
const BYTES_PER_PIXEL: usize = 4;

/// The buffers one display is driven from, and the flip that swaps them.
#[derive(Debug)]
pub struct Scanout {
    /// The connector, the CRTC and the plane a commit names.
    pipe: Pipe,
    /// The two buffers, in the order they were allocated.
    buffers: [DumbBuffer; BUFFERS],
    /// The framebuffer each buffer is registered as, at the same index.
    framebuffers: [Framebuffer; BUFFERS],
    /// Which buffer is off the screen, and therefore the one a frame is written into.
    back: usize,
    /// Whether a flip was asked for and has not yet reported back.
    ///
    /// While this holds, the buffer at `back` is the one still on the screen, so writing into it
    /// would tear.
    flipping: bool,
}

impl Scanout {
    /// Two buffers for `output`, both registered, with the mode set and the first one on screen.
    ///
    /// `bgra` says which order the frames handed to [`Scanout::present`] store their channels,
    /// which is what [`Pixels::is_bgra`] answers. It picks the fourcc: `XRGB8888` for bytes that
    /// are blue first, `XBGR8888` for bytes that are red first. Choosing the format is what makes
    /// a frame a copy rather than a swizzle of two million pixels.
    ///
    /// `commit` is the frame loop's own, and it is the same one every later flip goes through. An
    /// atomic commit holds the mode blob of every CRTC it has set and destroys the one it replaces;
    /// a commit made here and dropped here would leave the blob of this modeset behind, so a
    /// display re-created while the program runs would leak 68 bytes of kernel memory each time.
    ///
    /// The caller holds DRM master. A modeset from a process that is not the master is refused
    /// with `EPERM`, and taking master belongs to the frame loop: it owns the device for as long
    /// as the program runs, and this owns two buffers on it.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the driver refuses a buffer, a framebuffer or the
    /// modeset. Whatever was allocated before the refusal is released, so a failure here leaves
    /// the device as it was found.
    pub fn new(
        device: &Device,
        output: &Output,
        commit: &mut dyn Commit,
        bgra: bool,
    ) -> Result<Self, PlatformError> {
        let format = fourcc(bgra);
        let width = output.mode.width();
        let height = output.mode.height();

        let (front, front_id) = allocate(device, width, height, format)?;
        let (back, back_id) = match allocate(device, width, height, format) {
            Ok(pair) => pair,
            Err(error) => {
                drop(device.remove_framebuffer(front_id));
                drop(device.destroy_dumb_buffer(front));
                return Err(error);
            }
        };

        let scanout = Self {
            pipe: output.pipe,
            buffers: [front, back],
            framebuffers: [front_id, back_id],
            // The first buffer is what the modeset below puts on the screen, so the second is the
            // one the first frame is written into.
            back: 1,
            flipping: false,
        };

        if let Err(error) = commit.modeset(device, output.pipe, &output.mode, front_id) {
            scanout.release(device);
            return Err(backend(error));
        }
        Ok(scanout)
    }

    /// Copies `pixels` into the back buffer and flips to it, reporting whether it went.
    ///
    /// Answers `false` when a flip is still on its way. The back buffer is the one still on the
    /// screen until the completion arrives, so writing into it would tear, and the caller's frame
    /// is declined rather than shown torn. [`Scanout::drain`] clears that.
    ///
    /// The flip returns at once. What says the frame reached the screen is the completion event,
    /// which the loop reads off the device and hands back through [`Scanout::drain`].
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when `pixels` is not the extent of the buffers, when the
    /// buffer cannot be mapped, and when the driver refuses the flip.
    pub fn present(
        &mut self,
        device: &Device,
        commit: &mut dyn Commit,
        pixels: &Pixels,
    ) -> Result<bool, PlatformError> {
        if self.flipping {
            return Ok(false);
        }

        let back = self.back;
        let width = self.buffers[back].width();
        let height = self.buffers[back].height();
        let size = pixels.size();
        if u32::try_from(size.width) != Ok(width) || u32::try_from(size.height) != Ok(height) {
            return Err(PlatformError::Backend(format!(
                "a frame of {}x{} cannot be scanned out of a {width}x{height} buffer",
                size.width, size.height
            )));
        }

        // The driver rounds a row up, so the two strides differ and each side steps by its own.
        let destination_stride = self.buffers[back].stride() as usize;
        let source_stride = width as usize * BYTES_PER_PIXEL;
        let bytes = self.buffers[back].bytes(device).map_err(backend)?;
        blit(
            pixels.bytes(),
            source_stride,
            bytes,
            destination_stride,
            height as usize,
        );

        commit
            .flip(device, self.pipe, self.framebuffers[back])
            .map_err(backend)?;
        self.flipping = true;
        self.back = (back + 1) % BUFFERS;
        Ok(true)
    }

    /// Clears the outstanding flip when `events` says this display's flip finished.
    ///
    /// The loop reads the device once and hands every scanout the same slice, because one read
    /// carries the completions of every CRTC that finished. An event naming another CRTC is
    /// another display's, and this leaves it alone.
    pub fn drain(&mut self, events: &[Event]) {
        if completed(events, self.pipe.crtc) {
            self.flipping = false;
        }
    }

    /// Gives the framebuffers and the buffers back.
    ///
    /// Taken by value, because everything it holds is dead afterwards. A refusal is reported
    /// through the log rather than returned: this runs while a program is shutting down, where
    /// there is nothing a caller could do with it, and the rest still has to be released.
    ///
    /// The framebuffers go first. Removing one that an enabled plane is scanning out disables that
    /// plane, so the display goes dark on shutdown.
    pub fn release(self, device: &Device) {
        for framebuffer in self.framebuffers {
            if let Err(error) = device.remove_framebuffer(framebuffer) {
                let id = framebuffer.id();
                warn!(
                    "framebuffer {id} could not be removed, so it stays until the device \
                     closes: {error}"
                );
            }
        }
        for buffer in self.buffers {
            if let Err(error) = device.destroy_dumb_buffer(buffer) {
                warn!("a scanout buffer could not be released: {error}");
            }
        }
    }
}

/// Returns the fourcc whose bytes lie in the order the readback produced.
///
/// A fourcc names its channels most significant first inside one 32-bit word, and `drm_fourcc.h`
/// writes that out: `XRGB8888` is "[31:0] x:R:G:B 8:8:8:8 little endian". Little endian puts the
/// word's least significant byte first in memory, so `x:R:G:B` reaches memory as **B, G, R, x** —
/// which is a BGRA readback. `XBGR8888` is "[31:0] x:B:G:R", so **R, G, B, x** in memory, which is
/// an RGBA one.
///
/// Choosing here rather than swizzling is the point: a swizzle over a 1920x1080 frame is two
/// million operations a frame to reach a format the display could have been given directly.
///
/// The `X` form rather than the `A` form: the scanout ignores that byte, so a frame whose alpha is
/// anything other than opaque still reaches the screen as it was drawn.
fn fourcc(bgra: bool) -> Format {
    if bgra {
        Format::XRGB8888
    } else {
        Format::XBGR8888
    }
}

/// Copies `rows` rows from `source` into `destination`, each stepping by its own stride.
///
/// The source is tightly packed, so its stride is the row's own width in bytes. The destination is
/// the driver's buffer, whose stride is rounded up past that, and the bytes past the end of a row
/// are padding this leaves alone. A copy that stepped both by the same number would write a
/// diagonal.
///
/// **A short side truncates.** Whichever of the two runs out first ends the copy, and a row with
/// too little room is left out whole. So this can put part of a picture on a screen, and it can
/// panic on no input at all. [`Scanout::present`] refuses a frame of the wrong extent before it
/// reaches here; this one stays total, so that a mistake above it is a fault a person can see
/// rather than a crash inside a frame loop. A stride of zero copies nothing, for the same reason.
fn blit(
    source: &[u8],
    source_stride: usize,
    destination: &mut [u8],
    destination_stride: usize,
    rows: usize,
) {
    if source_stride == 0 || destination_stride == 0 {
        return;
    }
    // A destination narrower than the source is the same truncation one row along.
    let width = source_stride.min(destination_stride);
    for (into, from) in destination
        .chunks_exact_mut(destination_stride)
        .zip(source.chunks_exact(source_stride))
        .take(rows)
    {
        into[..width].copy_from_slice(&from[..width]);
    }
}

/// Returns `true` if any of `events` says a flip on `crtc` finished.
fn completed(events: &[Event], crtc: u32) -> bool {
    events.iter().any(
        |event| matches!(event, Event::FlipComplete { crtc: finished, .. } if *finished == crtc),
    )
}

/// Allocates one buffer of this extent, registered for scanout.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when the driver refuses either half. A buffer whose
/// framebuffer was refused is released here rather than left allocated until the device closes.
fn allocate(
    device: &Device,
    width: u32,
    height: u32,
    format: Format,
) -> Result<(DumbBuffer, Framebuffer), PlatformError> {
    let buffer = device
        .create_dumb_buffer(width, height, format)
        .map_err(backend)?;
    match device.add_framebuffer(&buffer, format) {
        Ok(framebuffer) => Ok((buffer, framebuffer)),
        Err(error) => {
            drop(device.destroy_dumb_buffer(buffer));
            Err(backend(error))
        }
    }
}

#[cfg(test)]
mod tests {
    //! The two decisions a device cannot help with: which fourcc a readback is, and the copy.
    //!
    //! The copy is where a display shows a diagonal, and it needs no hardware to prove: the source
    //! and the destination are slices, and the strides differ here exactly as they differ on a
    //! driver that rounds a row up.

    use super::{blit, completed, fourcc};
    use std::time::Duration;
    use zgui_drm::Event;
    use zgui_drm::format::Format;

    /// A destination of `rows` rows of `stride` bytes, filled with a byte no source writes.
    fn padded(stride: usize, rows: usize) -> Vec<u8> {
        vec![0xAA; stride * rows]
    }

    #[test]
    fn a_bgra_readback_is_scanned_out_as_the_fourcc_whose_bytes_are_blue_first() {
        assert_eq!(
            fourcc(true),
            Format::XRGB8888,
            "x:R:G:B is B, G, R, x in memory"
        );
        assert_eq!(fourcc(false), Format::XBGR8888, "and x:B:G:R is R, G, B, x");
    }

    #[test]
    fn a_source_narrower_than_the_destination_lands_one_row_per_row() {
        let source = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut destination = padded(6, 2);

        blit(&source, 4, &mut destination, 6, 2);

        assert_eq!(
            destination,
            [1, 2, 3, 4, 0xAA, 0xAA, 5, 6, 7, 8, 0xAA, 0xAA],
            "every row starts at its own stride, and the padding is left alone"
        );
    }

    #[test]
    fn two_equal_strides_copy_the_whole_of_the_source() {
        let source = [1, 2, 3, 4, 5, 6];
        let mut destination = padded(3, 2);

        blit(&source, 3, &mut destination, 3, 2);

        assert_eq!(destination, source, "nothing is left over on either side");
    }

    #[test]
    fn no_rows_copies_nothing() {
        let source = [1, 2, 3, 4];
        let mut destination = padded(4, 1);

        blit(&source, 4, &mut destination, 4, 0);

        assert_eq!(destination, [0xAA; 4], "a frame of no rows writes no bytes");
    }

    #[test]
    fn a_destination_too_short_takes_the_rows_that_fit_and_no_more() {
        // Three rows asked for, two rows of room. The alternative to truncating is an index past
        // the end, which in a frame loop is a panic per frame.
        let source = [1, 2, 3, 4, 5, 6];
        let mut destination = padded(4, 2);

        blit(&source, 2, &mut destination, 4, 3);

        assert_eq!(
            destination,
            [1, 2, 0xAA, 0xAA, 3, 4, 0xAA, 0xAA],
            "the rows that fit land whole, and the third is not written"
        );
    }

    #[test]
    fn a_source_too_short_supplies_the_rows_it_has() {
        let source = [1, 2, 3];
        let mut destination = padded(4, 3);

        blit(&source, 2, &mut destination, 4, 3);

        assert_eq!(
            destination,
            [
                1, 2, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA
            ],
            "one whole row is there, and the half row after it is not written"
        );
    }

    #[test]
    fn a_stride_of_zero_copies_nothing_rather_than_dividing_by_it() {
        let source = [1, 2, 3, 4];
        let mut destination = padded(4, 1);

        blit(&source, 0, &mut destination, 4, 1);
        blit(&source, 4, &mut destination, 0, 1);

        assert_eq!(destination, [0xAA; 4]);
    }

    /// A completion for `crtc`, as the device reports one.
    fn flip(crtc: u32) -> Event {
        Event::FlipComplete {
            crtc,
            at: Duration::from_secs(1),
            user_data: 0,
        }
    }

    #[test]
    fn a_completion_naming_this_crtc_is_this_displays_flip() {
        assert!(completed(&[flip(62)], 62));
        assert!(
            completed(&[flip(81), flip(62)], 62),
            "one read carries the completions of every display that finished"
        );
    }

    #[test]
    fn a_completion_naming_another_crtc_belongs_to_another_display() {
        assert!(!completed(&[flip(81)], 62));
        assert!(
            !completed(&[], 62),
            "and a read with nothing in it says nothing"
        );
    }
}
