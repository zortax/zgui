//! The display a frame is put on, and how a renderer finds the one it draws to.
//!
//! A renderer factory is handed a surface and a target and nothing else, and a surface says nothing
//! about the CRTC behind it. The frame loop holds the buffers, the mode and the flip, and it knows
//! which display each surface is. So the map goes in a [`Displays`]: the caller makes one, hands it
//! to [`run`](crate::run) and to the renderer it installs, and the loop writes the map into it
//! while it drives.
//!
//! The loop and the renderer are coupled through that map and through nothing else. Neither half
//! draws anything without the other, so `App::run_drm` makes one [`Displays`] and gives it to both.
//!
//! # The state the two halves share
//!
//! A [`Scanout`] is written by two callers at different moments. The loop clears its outstanding
//! flip when the device reports a completion; the renderer copies a frame into its back buffer and
//! asks for the next flip. Both run on the loop's thread, one at a time, so the buffers are held
//! through [`Rc`] and [`RefCell`] rather than through a lock.
//!
//! Every display on one device shares one commit, for the reason the loop holds one at all: an
//! atomic commit caches each object's properties and destroys the mode blob it replaces, so a
//! second commit would read the properties again and would leave the first one's blob behind.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_drm::Device;
use zgui_drm::commit::Commit;
use zgui_platform::{PlatformError, SurfaceId};
use zgui_render_wgpu::Pixels;

use crate::cursor::Cursor;
use crate::scanout::Scanout;

/// The displays a frame loop is driving, by the surface each is seen as.
///
/// The seam between the loop and what draws through it. A renderer holds one of these and asks it
/// which display a surface is; the loop writes the answers in when it lights the displays, and
/// takes them back when it stops. Empty before the first display is lit and after the last frame,
/// so a renderer with no loop behind it fails where it is called.
///
/// Cloning costs one reference count, and every clone reads the same map. Both ends run on the
/// thread the loop turns on, one at a time, so the map is held through [`Rc`] and [`RefCell`]
/// rather than through a lock.
#[derive(Clone, Default)]
pub struct Displays {
    /// What the loop wrote in, by surface.
    driven: Rc<RefCell<Vec<(SurfaceId, DrmDisplay)>>>,
}

impl Displays {
    /// Creates a map no loop has written to yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the display the surface numbered `id` draws to, when a loop is driving one.
    ///
    /// `None` says the surface belongs to no loop holding this map. A renderer installed without
    /// [`run`](crate::run) gets that answer, and it is reported rather than worked around: a
    /// renderer that composed a frame and put it nowhere is a program that appears to run and shows
    /// nothing.
    #[must_use]
    pub fn for_surface(&self, id: SurfaceId) -> Option<DrmDisplay> {
        find(&self.driven.borrow(), id)
    }

    /// Writes `displays` in, and takes them back when the returned value is dropped.
    ///
    /// Replaces whatever was there before. One loop drives one map, so anything already in it
    /// belongs to a loop that has already finished.
    pub(crate) fn drive(&self, displays: Vec<(SurfaceId, DrmDisplay)>) -> Driving {
        *self.driven.borrow_mut() = displays;
        Driving(self.clone())
    }
}

/// One display, as the thing a frame is put on.
///
/// A cheap handle: cloning it costs three reference counts. What it names is one display's pair of
/// buffers, the pointer that is drawn over them, the device they live on, and the commit every
/// flip on that device goes through.
#[derive(Clone)]
pub struct DrmDisplay {
    /// The device the display hangs off, kept open for as long as anything draws to it.
    device: Arc<Device>,
    /// The commit every flip on this device goes through, shared by every display it drives.
    commit: Rc<RefCell<Box<dyn Commit>>>,
    /// The two buffers this display is driven from, shared with the loop that drains its flips.
    scanout: Rc<RefCell<Scanout>>,
    /// The pointer on this display, shared with the loop that decides where it is.
    ///
    /// Here for the same reason the buffers are. `Surface::set_cursor` cannot reach a display: a
    /// surface is `Send + Sync` and this handle is neither, so the shape a surface was asked for
    /// travels to the loop as a value on the surface. What the loop cannot do for itself is draw
    /// the pointer into a frame, because the frame arrives here — so the loop writes the cursor
    /// and this reads it, which is the arrangement the scanout already has.
    cursor: Rc<RefCell<Cursor>>,
}

impl DrmDisplay {
    /// Creates the display `scanout` drives, flipped through `commit` on `device`, with `cursor`
    /// over it.
    ///
    /// The frame loop calls this. It owns all four, and it puts the result in a [`Displays`] so
    /// that the renderer can find it.
    pub fn new(
        device: Arc<Device>,
        commit: Rc<RefCell<Box<dyn Commit>>>,
        scanout: Rc<RefCell<Scanout>>,
        cursor: Rc<RefCell<Cursor>>,
    ) -> Self {
        Self {
            device,
            commit,
            scanout,
            cursor,
        }
    }

    /// Returns `true` if this display's own frames carry the pointer.
    ///
    /// True where the device offered no cursor plane. A renderer reads it to decide what to do
    /// with a frame that damaged nothing: on a display with a plane such a frame is worth nothing,
    /// and on this one it is the only thing that moves the pointer, because the pointer is drawn
    /// into the frame.
    pub fn carries_the_pointer(&self) -> bool {
        !self.cursor.borrow().on_a_plane()
    }

    /// Copies `pixels` into the back buffer, draws the pointer over it, and flips to it.
    ///
    /// Answers `false` while a flip is still on its way, which is [`Scanout::present`]'s own
    /// answer: the back buffer is the one still on the screen until the completion arrives, so the
    /// frame is declined rather than shown torn.
    ///
    /// The pointer is drawn only where this display has no cursor plane. Where it has one the
    /// display engine composites it, and a frame that drew it as well would show two.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when `pixels` is not the extent of the display, when the
    /// buffer cannot be mapped, and when the driver refuses the flip.
    pub fn present(&self, pixels: &Pixels) -> Result<bool, PlatformError> {
        let mut commit = self.commit.borrow_mut();
        self.scanout.borrow_mut().present(
            &self.device,
            &mut **commit,
            pixels,
            &self.cursor.borrow(),
        )
    }
}

/// The displays a loop wrote in, taken back when this is dropped.
///
/// Held by the frame loop for exactly as long as it runs. Nothing is written before the first
/// surface exists and nothing stays after the last frame, so a renderer outliving its loop finds no
/// display rather than a stale one. That is also what releases the loop's hold on the buffers: a
/// scanout is given back by value, so the displays naming it go first.
#[must_use = "the displays are readable only for as long as this is held"]
pub(crate) struct Driving(Displays);

impl Drop for Driving {
    fn drop(&mut self) {
        self.0.driven.borrow_mut().clear();
    }
}

/// Returns the entry `id` names, out of the ones a loop wrote in.
///
/// Written over any value so that what it answers can be asserted with no device open: a display
/// holds two buffers a driver allocated, and the lookup holds nothing at all.
fn find<T: Clone>(driven: &[(SurfaceId, T)], id: SurfaceId) -> Option<T> {
    driven
        .iter()
        .find(|(driving, _)| *driving == id)
        .map(|(_, display)| display.clone())
}

#[cfg(test)]
mod tests {
    //! Which display a surface reaches, which is the one decision here that can be got wrong.

    use super::{Displays, find};
    use zgui_platform::SurfaceId;

    #[test]
    fn a_surface_reaches_the_display_that_was_published_under_its_own_number() {
        let published = [(SurfaceId::new(1), "first"), (SurfaceId::new(2), "second")];

        assert_eq!(find(&published, SurfaceId::new(1)), Some("first"));
        assert_eq!(
            find(&published, SurfaceId::new(2)),
            Some("second"),
            "a display is never the one beside it"
        );
    }

    #[test]
    fn a_surface_nothing_published_reaches_no_display() {
        let published = [(SurfaceId::new(1), "first")];

        assert_eq!(find(&published, SurfaceId::new(7)), None);
        assert_eq!(find::<&str>(&[], SurfaceId::new(1)), None);
    }

    #[test]
    fn a_surface_reaches_nothing_while_no_loop_is_driving() {
        // What a renderer installed without `run` sees. It fails where it is called rather than
        // composing a frame and putting it nowhere.
        assert!(Displays::new().for_surface(SurfaceId::new(1)).is_none());
    }
}
