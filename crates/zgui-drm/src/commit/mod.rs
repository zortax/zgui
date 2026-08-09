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

use crate::cursor::{CursorImage, CursorPlane};
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

    /// Puts `image` on `plane`, with its top left corner at `x`, `y` on the CRTC.
    ///
    /// The position is the image's top left corner on both interfaces, so a caller that wants the
    /// pointer at a point puts the image at that point less its hotspot. That leaves a pointer
    /// near the left or the top edge at a negative coordinate, which both interfaces take: the
    /// position is a signed field on one and a signed range property on the other.
    ///
    /// [`CursorImage`] carries a framebuffer id and a GEM handle because the two interfaces name
    /// the buffer differently. This decides which of the two is read, and which layouts are
    /// allowed: the legacy interface reads a buffer with a format and a stride of its own, and
    /// refuses an image that disagrees. [`CursorImage`] states both rules.
    ///
    /// The atomic interface tests this configuration before it applies it, the way
    /// [`Commit::modeset`] does. This is the commit that turns the cursor plane on, so it is the
    /// one a driver is most likely to refuse, and a refusal leaves the display as it was.
    ///
    /// This blocks for up to two vertical blanks on the atomic interface, which
    /// [`AtomicCommit::set_cursor`] sets out. A caller reaches it when the cursor's shape changes;
    /// a motion event costs [`Commit::move_cursor`] instead.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the kernel refuses the image, which is
    /// how a buffer of a size the driver will not take is reported.
    /// [`Error::Unusable`](crate::Error::Unusable) covers what the interfaces refuse for
    /// themselves: the atomic one when [`CursorPlane::id`] or [`CursorImage::framebuffer`] is
    /// `None`, and the legacy one when the image's format or stride is not what it would read.
    fn set_cursor(
        &mut self,
        device: &Device,
        plane: CursorPlane,
        image: CursorImage,
        x: i32,
        y: i32,
    ) -> Result<()>;

    /// Moves the cursor already on `plane` to `x`, `y`, leaving its image where it is.
    ///
    /// A pointer costs this per motion event: no buffer is touched and no frame is drawn.
    ///
    /// A [`Commit::set_cursor`] has to have put an image on `plane` first. Neither interface
    /// reports a caller that skipped it: the atomic one commits two properties of a plane linked
    /// to no CRTC, which reaches no CRTC and is accepted having done nothing, and the legacy one
    /// moves a cursor the CRTC does not have. Nothing in this crate or in the kernel checks the
    /// rule, so a caller keeps it by reading this.
    ///
    /// # Both interfaces send `DRM_IOCTL_MODE_CURSOR2`
    ///
    /// The kernel gives that request a shortcut the atomic ioctl has no way to ask for, so it is
    /// the cheaper of the two per motion on an atomic device as well as on a legacy one.
    /// [`AtomicCommit::move_cursor`] sets out which waits the shortcut skips, which one survives,
    /// and what mixing the two interfaces on one plane costs.
    ///
    /// It can still wait. A move issued while a flip is outstanding on the same CRTC waits for
    /// that flip, which is one refresh, and a driver that takes the asynchronous plane path waits
    /// for nothing. A frame loop that reads flip completions, deadlines and input on one thread
    /// does none of the three while it waits, so a caller that owns such a loop moves the cursor
    /// once a turn, however many motion events the turn collected.
    ///
    /// # Errors
    ///
    /// The ones [`Commit::set_cursor`] returns, other than the ones about the image.
    fn move_cursor(&mut self, device: &Device, plane: CursorPlane, x: i32, y: i32) -> Result<()>;

    /// Takes the cursor off `plane`.
    ///
    /// The buffer behind the image stays allocated, so the same image can be put back.
    ///
    /// # Errors
    ///
    /// The ones [`Commit::set_cursor`] returns.
    fn hide_cursor(&mut self, device: &Device, plane: CursorPlane) -> Result<()>;
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
