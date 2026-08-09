//! The legacy interface: set a CRTC, flip a page.
//!
//! This exists for drivers with no atomic interface. It cannot ask whether a configuration is
//! possible without applying it, and it cannot carry a fence. So a caller on this path learns that
//! a mode is impossible from the commit that tried to set it, and waits on the graphics device
//! before every flip.
//!
//! This path runs on ordinary modern hardware as well, because the legacy ioctls do not go away
//! when a driver gains an atomic interface: the kernel implements them over its own atomic
//! helpers. So a device opened with [`Interface::Legacy`](crate::device::Interface::Legacy) serves
//! these requests, whatever the driver offers.
//!
//! That leaves out a driver whose legacy path is native code under the ioctls. Almost nothing is
//! any more, and this crate has no way to reach one.

use crate::commit::{Commit, Pipe};
use crate::cursor::{CursorImage, CursorPlane};
use crate::device::Device;
use crate::error::{Error, Result};
use crate::framebuffer::Framebuffer;
use crate::ioctl;
use crate::resources::Mode;
use crate::sys;

/// The legacy commit interface.
///
/// It holds nothing. Each call is one ioctl that names everything it needs, so there is no set of
/// property ids to keep and nothing to read once.
///
/// ```
/// use zgui_drm::Commit;
/// use zgui_drm::commit::LegacyCommit;
///
/// assert!(
///     !LegacyCommit::new().can_test(),
///     "this interface applies a configuration to find out whether the hardware takes it",
/// );
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct LegacyCommit;

impl LegacyCommit {
    /// Creates the legacy commit interface.
    pub fn new() -> Self {
        Self
    }
}

impl Commit for LegacyCommit {
    fn can_test(&self) -> bool {
        false
    }

    fn modeset(
        &mut self,
        device: &Device,
        pipe: Pipe,
        mode: &Mode,
        framebuffer: Framebuffer,
    ) -> Result<()> {
        // The kernel takes a list of connectors to route to this CRTC. One display is one
        // connector, and the array has to outlive the call.
        let mut connectors = [pipe.connector];
        let mut request = sys::drm_mode_crtc {
            set_connectors_ptr: connectors.as_mut_ptr() as u64,
            count_connectors: 1,
            crtc_id: pipe.crtc,
            fb_id: framebuffer.id(),
            // Scanout starts at the top left of the framebuffer.
            x: 0,
            y: 0,
            // The gamma table size. `drm_mode_getcrtc` fills this in when a CRTC is read, and
            // `drm_mode_setcrtc` reads it nowhere.
            gamma_size: 0,
            // `mode` is read only where this says it holds a mode.
            mode_valid: 1,
            mode: mode.raw,
        };
        ioctl::issue(device.fd(), ioctl::MODE_SETCRTC, &mut request)
    }

    fn flip(&mut self, device: &Device, pipe: Pipe, framebuffer: Framebuffer) -> Result<()> {
        let mut request = sys::drm_mode_crtc_page_flip {
            crtc_id: pipe.crtc,
            fb_id: framebuffer.id(),
            // Without the event flag nothing ever tells a frame loop the old buffer is free again.
            flags: sys::DRM_MODE_PAGE_FLIP_EVENT,
            // The header states that this must be zero.
            reserved: 0,
            // Returned in the flip event. Zero: the event already names the CRTC, and that is how
            // a caller tells one pipe from another.
            user_data: 0,
        };
        ioctl::issue(device.fd(), ioctl::MODE_PAGE_FLIP, &mut request)
    }

    fn set_cursor(
        &mut self,
        device: &Device,
        plane: CursorPlane,
        image: CursorImage,
        x: i32,
        y: i32,
    ) -> Result<()> {
        // The kernel reads this buffer with a format and a stride of its own choosing, so it
        // accepts an image laid out any other way and reinterprets it. This is where that becomes
        // an error a caller can read.
        layout(image)?;

        // The buffer and the position in one request. The two flags are separate so that a move
        // costs no buffer change, and both are set here so the image appears where the caller
        // asked: `drm_mode_cursor_universal` reads `crtc->cursor_x` and `crtc->cursor_y` for a
        // request that carries no `CURSOR_MOVE`, and those hold where the last cursor on this CRTC
        // was.
        //
        // `handle` is the GEM handle the driver allocated the buffer as, which the header calls a
        // driver specific handle. `CursorPlane::id` is read by the atomic interface alone, and
        // this request names the CRTC.
        cursor(
            device,
            sys::drm_mode_cursor2 {
                flags: sys::DRM_MODE_CURSOR_BO | sys::DRM_MODE_CURSOR_MOVE,
                crtc_id: plane.crtc,
                x,
                y,
                width: image.width,
                height: image.height,
                handle: image.handle,
                // The one thing this interface carries and the atomic property set does not. A
                // para-virtualised driver relays it to the host that draws the pointer.
                hot_x: image.hotspot_x,
                hot_y: image.hotspot_y,
            },
        )
    }

    fn move_cursor(&mut self, device: &Device, plane: CursorPlane, x: i32, y: i32) -> Result<()> {
        // Without `DRM_MODE_CURSOR_BO` the kernel reads no buffer field and keeps the image the
        // CRTC already has, which is what makes a move as cheap as it is.
        cursor(
            device,
            sys::drm_mode_cursor2 {
                flags: sys::DRM_MODE_CURSOR_MOVE,
                crtc_id: plane.crtc,
                x,
                y,
                ..Default::default()
            },
        )
    }

    fn hide_cursor(&mut self, device: &Device, plane: CursorPlane) -> Result<()> {
        // A handle of zero with `DRM_MODE_CURSOR_BO` is how this interface spells "no cursor", and
        // the header says so. The position is left alone, so the image can be put back where it
        // was.
        cursor(
            device,
            sys::drm_mode_cursor2 {
                flags: sys::DRM_MODE_CURSOR_BO,
                crtc_id: plane.crtc,
                handle: 0,
                ..Default::default()
            },
        )
    }
}

/// Refuses an image this interface would read as something else.
///
/// `drm_mode_cursor2` carries no format and no stride, and the kernel substitutes its own for
/// both. So an image that disagrees with them reaches the display reinterpreted while every return
/// value stays `Ok`, which is the one failure nothing downstream can see. The two rules are stated
/// on [`CursorImage::LEGACY_FORMAT`] and [`CursorImage::legacy_stride`], and neither is in a
/// vendored header — both are read out of the kernel and held here by hand.
///
/// # Errors
///
/// Returns [`Error::Unusable`] naming which of the two the image breaks.
fn layout(image: CursorImage) -> Result<()> {
    if image.format != CursorImage::LEGACY_FORMAT {
        return Err(Error::Unusable(format!(
            "a cursor on the legacy interface is read as {:?}, so {:?} reaches the display \
             reinterpreted: an unused byte becomes the alpha channel, and a zero there is a \
             cursor nobody can see",
            CursorImage::LEGACY_FORMAT,
            image.format,
        )));
    }

    let rows = CursorImage::legacy_stride(image.width);
    if u64::from(image.stride) != rows {
        return Err(Error::Unusable(format!(
            "a cursor {} pixels wide is read a row every {rows} bytes on the legacy interface, \
             and this buffer has a stride of {}, so the kernel would read it sheared",
            image.width, image.stride,
        )));
    }

    Ok(())
}

/// Issues one cursor request.
///
/// # Errors
///
/// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the kernel refuses.
fn cursor(device: &Device, mut request: sys::drm_mode_cursor2) -> Result<()> {
    ioctl::issue(device.fd(), ioctl::MODE_CURSOR2, &mut request)
}

#[cfg(test)]
mod tests {
    //! The layouts this interface can carry, which is the check no device is needed for.

    use super::*;

    use crate::format::Format;

    /// Returns an image the legacy interface reads the way it was written.
    fn image() -> CursorImage {
        CursorImage {
            framebuffer: None,
            handle: 7,
            width: 64,
            height: 64,
            stride: 256,
            format: Format::ARGB8888,
            hotspot_x: 0,
            hotspot_y: 0,
        }
    }

    #[test]
    fn a_row_of_a_legacy_cursor_is_four_bytes_a_pixel_with_no_rounding() {
        assert_eq!(CursorImage::legacy_stride(0), 0);
        assert_eq!(CursorImage::legacy_stride(64), 256);
        assert_eq!(CursorImage::legacy_stride(256), 1024);
        // Answering a `u64` leaves room for a width no buffer could have, so it is compared at
        // its true size.
        assert_eq!(CursorImage::legacy_stride(u32::MAX), 17_179_869_180);
    }

    #[test]
    fn an_image_laid_out_the_way_this_interface_reads_it_is_taken() {
        assert!(layout(image()).is_ok());
    }

    #[test]
    fn an_image_in_another_format_is_refused_rather_than_shown_transparent() {
        // `XRGB8888` is what everything else in this crate scans out, so this is the mistake a
        // caller actually makes. Read as `ARGB8888`, its unused byte is the alpha channel and the
        // cursor is invisible with every call reporting success.
        let error = layout(CursorImage {
            format: Format::XRGB8888,
            ..image()
        })
        .expect_err("a format this interface would reinterpret is refused");
        assert!(
            matches!(&error, Error::Unusable(what) if what.contains("reinterpreted")),
            "the refusal says what the kernel would do with it: {error}"
        );
    }

    #[test]
    fn an_image_whose_rows_the_driver_rounded_up_is_refused_rather_than_shown_sheared() {
        // A driver rounds a dumb buffer's stride up for its own reasons, and this interface reads
        // four bytes a pixel whatever it is told.
        let error = layout(CursorImage {
            stride: 512,
            ..image()
        })
        .expect_err("a stride this interface would read past is refused");
        assert!(
            matches!(&error, Error::Unusable(what) if what.contains("sheared")),
            "the refusal says what the kernel would do with it: {error}"
        );
    }

    #[test]
    fn an_image_smaller_than_the_buffer_it_sits_in_is_refused() {
        // A 32x32 arrow allocated inside a 256x256 cursor buffer keeps the buffer's stride, and
        // the two disagree by a factor of eight. The stride rule catches it.
        assert!(
            layout(CursorImage {
                width: 32,
                height: 32,
                stride: 1024,
                ..image()
            })
            .is_err(),
            "an image narrower than its buffer is caught by the stride it kept"
        );
    }
}
