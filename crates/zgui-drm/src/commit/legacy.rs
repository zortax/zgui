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
use crate::device::Device;
use crate::error::Result;
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
}
