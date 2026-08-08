//! The display a frame is put on, and how a renderer finds the one it draws to.
//!
//! A renderer factory is handed a surface and a target and nothing else, and a surface says nothing
//! about the CRTC behind it. The frame loop holds the buffers, the mode and the flip, and it knows
//! which display each surface is. So the loop publishes that map here, on the thread it runs on,
//! and the factory reads it.
//!
//! The loop and the renderer are coupled through that map and through nothing else. Neither half
//! draws anything without the other, so `App::run_drm` installs both together and offers no way to
//! install one.
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

use crate::scanout::Scanout;

thread_local! {
    /// The displays the frame loop on this thread is driving, by the surface each is seen as.
    ///
    /// Empty whenever no loop is running, which is what makes a factory installed without one fail
    /// where it is called rather than draw into nothing.
    static DRIVEN: RefCell<Vec<(SurfaceId, DrmDisplay)>> = const { RefCell::new(Vec::new()) };
}

/// One display, as the thing a frame is put on.
///
/// A cheap handle: cloning it costs two reference counts. What it names is one display's pair of
/// buffers, the device they live on, and the commit every flip on that device goes through.
#[derive(Clone)]
pub struct DrmDisplay {
    /// The device the display hangs off, kept open for as long as anything draws to it.
    device: Arc<Device>,
    /// The commit every flip on this device goes through, shared by every display it drives.
    commit: Rc<RefCell<Box<dyn Commit>>>,
    /// The two buffers this display is driven from, shared with the loop that drains its flips.
    scanout: Rc<RefCell<Scanout>>,
}

impl DrmDisplay {
    /// Creates the display `scanout` drives, flipped through `commit` on `device`.
    ///
    /// The frame loop calls this. It owns all three, and it publishes the result with
    /// `Driving::over` so that the renderer factory can find it.
    pub fn new(
        device: Arc<Device>,
        commit: Rc<RefCell<Box<dyn Commit>>>,
        scanout: Rc<RefCell<Scanout>>,
    ) -> Self {
        Self {
            device,
            commit,
            scanout,
        }
    }

    /// Copies `pixels` into the back buffer and flips to it, reporting whether it went.
    ///
    /// Answers `false` while a flip is still on its way, which is [`Scanout::present`]'s own
    /// answer: the back buffer is the one still on the screen until the completion arrives, so the
    /// frame is declined rather than shown torn.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when `pixels` is not the extent of the display, when the
    /// buffer cannot be mapped, and when the driver refuses the flip.
    pub fn present(&self, pixels: &Pixels) -> Result<bool, PlatformError> {
        let mut commit = self.commit.borrow_mut();
        self.scanout
            .borrow_mut()
            .present(&self.device, &mut **commit, pixels)
    }
}

/// The displays a loop published, taken back when this is dropped.
///
/// Held by the frame loop for exactly as long as it runs. Nothing is published before the first
/// surface exists and nothing stays published after the last frame, so a renderer built outside a
/// running loop finds no display rather than a stale one.
#[must_use = "the displays are published only for as long as this is held"]
pub(crate) struct Driving;

impl Driving {
    /// Publishes `displays` on this thread.
    ///
    /// Replaces whatever was published before. One frame loop runs per thread, so the previous
    /// contents belong to a loop that has already finished.
    pub(crate) fn over(displays: Vec<(SurfaceId, DrmDisplay)>) -> Self {
        DRIVEN.with_borrow_mut(|driven| *driven = displays);
        Self
    }
}

impl Drop for Driving {
    fn drop(&mut self) {
        DRIVEN.with_borrow_mut(Vec::clear);
    }
}

/// The display the surface numbered `id` draws to, when a frame loop is driving one.
///
/// `None` says the surface belongs to no loop on this thread, which is what a renderer factory
/// installed without `run` looks like. It is reported rather than worked around: a renderer that
/// composed a frame and put it nowhere is a program that appears to run and shows nothing.
pub fn driven(id: SurfaceId) -> Option<DrmDisplay> {
    DRIVEN.with_borrow(|driven| find(driven, id))
}

/// Returns the entry `id` names, out of the ones a loop published.
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

    use super::{driven, find};
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
    fn a_surface_reaches_nothing_while_no_loop_is_running() {
        // What a renderer factory installed without `run` sees. It is an error there rather than a
        // renderer that composes a frame and puts it nowhere.
        assert!(driven(SurfaceId::new(1)).is_none());
    }
}
