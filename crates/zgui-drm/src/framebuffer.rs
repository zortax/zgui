//! Registering a buffer as something a CRTC can scan out.

use crate::buffer::DumbBuffer;
use crate::device::Device;
use crate::error::Result;
use crate::format::{Format, Modifier};
use crate::ioctl;
use crate::sys;

/// A buffer the device has accepted for scanout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Framebuffer(pub(crate) u32);

impl Framebuffer {
    /// Returns the id, for naming this framebuffer in a commit.
    pub fn id(self) -> u32 {
        self.0
    }
}

impl Device {
    /// Registers a dumb buffer for scanout.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the driver will not scan this out — a
    /// format it does not take, or an extent past what the hardware allows.
    pub fn add_framebuffer(&self, buffer: &DumbBuffer, format: Format) -> Result<Framebuffer> {
        self.add_framebuffer_from_handles(
            buffer.width(),
            buffer.height(),
            format,
            [buffer.handle(), 0, 0, 0],
            [buffer.stride(), 0, 0, 0],
            [0; 4],
            None,
        )
    }

    /// Registers a buffer named by its GEM handles for scanout.
    ///
    /// The general form, which an imported dma-buf descriptor goes through. `modifier` is stated
    /// only when the caller knows the layout; passing nothing lets the driver assume its own, and
    /// a dumb buffer needs that. [`Modifier::INVALID`] means the same as passing nothing, because
    /// a graphics interface reports a layout it cannot name that way.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the driver refuses.
    #[expect(
        clippy::too_many_arguments,
        reason = "this is the kernel's own parameter list, and naming a struct for it would put \
                  a second spelling of `drm_mode_fb_cmd2` beside the generated one"
    )]
    pub fn add_framebuffer_from_handles(
        &self,
        width: u32,
        height: u32,
        format: Format,
        handles: [u32; 4],
        strides: [u32; 4],
        offsets: [u32; 4],
        modifier: Option<Modifier>,
    ) -> Result<Framebuffer> {
        let stated = modifier.filter(|modifier| *modifier != Modifier::INVALID);
        let modifiers = stated.map_or([0; 4], |modifier| {
            // The kernel takes one modifier per plane and requires them all to be equal. Every
            // entry that has no handle stays zero, as the header requires.
            let mut per_plane = [0_u64; 4];
            for (slot, handle) in per_plane.iter_mut().zip(handles) {
                if handle != 0 {
                    *slot = modifier.0;
                }
            }
            per_plane
        });

        let mut request = sys::drm_mode_fb_cmd2 {
            width,
            height,
            pixel_format: format.0,
            // The kernel reads `modifier` only when this flag is set, so a stated
            // `Modifier::LINEAR` — which is zero — still has to raise it.
            flags: if stated.is_some() {
                sys::DRM_MODE_FB_MODIFIERS
            } else {
                0
            },
            handles,
            pitches: strides,
            offsets,
            modifier: modifiers,
            ..Default::default()
        };
        ioctl::issue(self.fd(), ioctl::MODE_ADDFB2, &mut request)?;
        Ok(Framebuffer(request.fb_id))
    }

    /// Releases a framebuffer.
    ///
    /// The buffer behind it stays alive until its own handle is released. Removing a framebuffer
    /// that an enabled plane is scanning out disables that plane, and can disable the CRTC the
    /// plane is linked to, so a caller that wants the picture to stay up flips something else in
    /// first.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the driver refuses.
    pub fn remove_framebuffer(&self, framebuffer: Framebuffer) -> Result<()> {
        let mut id = framebuffer.0;
        ioctl::issue(self.fd(), ioctl::MODE_RMFB, &mut id)
    }
}
