//! Applying a display configuration.
//!
//! The kernel offers two interfaces, and they are different models. The atomic one describes a
//! whole configuration as object properties and applies it in a single call; such a call can be
//! tested before it is applied, and it can carry a fence. The legacy one sets a CRTC and flips a
//! page, and does neither.
//!
//! Which one a device uses is settled when the device is opened, and [`Device::is_atomic`] reports
//! it. [`for_device`] builds the implementation that answer names, and everything above this
//! module is written against [`Commit`] alone.

mod atomic;
mod legacy;

pub use crate::commit::atomic::AtomicCommit;
pub use crate::commit::legacy::LegacyCommit;

use crate::device::Device;
use crate::error::Result;
use crate::framebuffer::Framebuffer;
use crate::resources::Mode;

/// One display: a connector, the CRTC driving it, and the plane it scans out from.
#[derive(Debug, Clone, Copy)]
pub struct Pipe {
    /// The connector a display is plugged into.
    pub connector: u32,
    /// The CRTC driving it.
    pub crtc: u32,
    /// The primary plane it scans out from.
    ///
    /// Zero on a device with no universal planes. The kernel exposes only overlay planes to a
    /// client that did not set `DRM_CLIENT_CAP_UNIVERSAL_PLANES`, so there is no primary plane
    /// object to name, and the legacy interface addresses the CRTC instead.
    pub plane: u32,
}

/// The modesetting interface a device is driven through.
pub trait Commit {
    /// Returns `true` when a configuration can be tested before it is applied.
    ///
    /// True on the atomic interface and false on the legacy one. A caller that cannot test has to
    /// be ready for [`Commit::modeset`] to fail after it has already changed something.
    fn can_test(&self) -> bool;

    /// Puts `pipe` into `mode`, scanning out `framebuffer`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the kernel refuses the configuration,
    /// and [`Error::Unusable`](crate::Error::Unusable) from the atomic interface when an object
    /// lacks a property the commit needs.
    fn modeset(
        &mut self,
        device: &Device,
        pipe: Pipe,
        mode: &Mode,
        framebuffer: Framebuffer,
    ) -> Result<()>;

    /// Shows `framebuffer` on `pipe` at the next vertical blank.
    ///
    /// Returns without waiting. Completion arrives as an event on the device, which Task 12's
    /// reader picks up.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the kernel refuses the flip, which is
    /// what asking for a second flip before the first completed looks like.
    fn flip(&mut self, device: &Device, pipe: Pipe, framebuffer: Framebuffer) -> Result<()>;
}

/// Returns the commit interface `device` is driven through.
///
/// [`Device::is_atomic`] decided that when the device was opened, and this reads the answer. So a
/// caller builds one of these per device and keeps it.
///
/// ```no_run
/// use zgui_drm::Device;
/// use zgui_drm::commit::for_device;
///
/// let device = Device::open_first()?;
/// let commit = for_device(&device);
///
/// assert_eq!(
///     commit.can_test(),
///     device.is_atomic(),
///     "only the atomic interface tests a configuration before it applies it",
/// );
/// # Ok::<(), zgui_drm::Error>(())
/// ```
pub fn for_device(device: &Device) -> Box<dyn Commit> {
    if device.is_atomic() {
        Box::new(AtomicCommit::new())
    } else {
        Box::new(LegacyCommit::new())
    }
}
