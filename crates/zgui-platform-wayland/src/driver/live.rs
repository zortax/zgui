//! The part of the loop's state the borrowed context is allowed to change.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use zgui_platform::{ColorScheme, MonitorInfo, PlatformCapabilities, SurfaceId};

use crate::clock::SystemClock;
use crate::surface::WaylandSurface;
use crate::waker::PingWaker;

/// What a callback may change while the loop is inside it.
///
/// The loop's protocol state is held by value and mutated only while the toolkit is dispatching
/// into it. Everything here is different: it is reached through the borrowed context the
/// application is handed, which the contract gives out as `&self`, so each field carries the cell
/// that makes it writable there.
///
/// Keeping the two apart is what lets a protocol event and an application callback never overlap.
/// A dispatch writes protocol state; a delivery writes this; and neither runs inside the other,
/// because events are collected during a dispatch and delivered after it has finished.
pub struct Live {
    /// Every surface that exists.
    pub(crate) surfaces: RefCell<Vec<Arc<WaylandSurface>>>,
    /// Surfaces the application has closed, waiting for the end of the turn.
    ///
    /// A surface is not destroyed inside the callback that closed it: the objects underneath are
    /// the ones the loop is currently dispatching for, and taking them down there is a use after
    /// free. They are retired between turns, with nothing borrowed.
    pub(crate) retiring: RefCell<Vec<Arc<WaylandSurface>>>,
    /// Whether the loop has been asked to finish.
    pub(crate) exiting: Cell<bool>,
    /// The number the next surface gets.
    next: Cell<u64>,
    /// What is known about the outputs.
    pub(crate) monitors: RefCell<Vec<MonitorInfo>>,
    /// The identifier the desktop groups this application's windows under.
    ///
    /// Taken from whichever window states one first, because a request to activate names the
    /// application rather than the window: several compositors flash the task-bar entry it
    /// identifies even when they refuse the activation itself.
    pub(crate) app_id: RefCell<Option<String>>,
    /// The desktop's light or dark preference, where it could be discovered.
    ///
    /// Behind a lock rather than a cell, because the portal answers on a thread of its own: the
    /// bus is a socket like any other, and the loop's thread waits on nothing.
    pub(crate) scheme: Arc<std::sync::Mutex<Option<ColorScheme>>>,
    /// What this compositor turned out to be able to do.
    pub(crate) capabilities: PlatformCapabilities,
    /// Where the time comes from.
    pub(crate) clock: Arc<SystemClock>,
    /// How another thread reaches this loop.
    pub(crate) waker: Arc<PingWaker>,
}

impl Live {
    /// The state a loop starts with: no surfaces, nothing closed, nothing known.
    pub(crate) fn new(capabilities: PlatformCapabilities, waker: Arc<PingWaker>) -> Self {
        Self {
            surfaces: RefCell::new(Vec::new()),
            retiring: RefCell::new(Vec::new()),
            exiting: Cell::new(false),
            next: Cell::new(1),
            monitors: RefCell::new(Vec::new()),
            app_id: RefCell::new(None),
            scheme: Arc::default(),
            capabilities,
            clock: Arc::new(SystemClock::new()),
            waker,
        }
    }

    /// The number for one more surface.
    ///
    /// Never reused, so that a stale identifier held by something that outlived its surface names
    /// nothing rather than naming whichever window opened next.
    pub(crate) fn next_id(&self) -> SurfaceId {
        let id = self.next.get();
        self.next.set(id + 1);
        SurfaceId::new(id)
    }

    /// The surface with this number, while it still exists.
    pub(crate) fn surface(&self, id: SurfaceId) -> Option<Arc<WaylandSurface>> {
        self.surfaces
            .borrow()
            .iter()
            .find(|surface| zgui_platform::Surface::id(surface.as_ref()) == id)
            .map(Arc::clone)
    }

    /// Every surface that exists, as a snapshot.
    ///
    /// A copy rather than a borrow, because the caller is the application and what it does with
    /// the answer includes closing one of them.
    pub(crate) fn all(&self) -> Vec<Arc<WaylandSurface>> {
        self.surfaces.borrow().clone()
    }

    /// Moves a surface to the retiring list, if it is still here.
    pub(crate) fn close(&self, id: SurfaceId) {
        let mut surfaces = self.surfaces.borrow_mut();
        let Some(index) = surfaces
            .iter()
            .position(|surface| zgui_platform::Surface::id(surface.as_ref()) == id)
        else {
            return;
        };
        self.retiring.borrow_mut().push(surfaces.remove(index));
    }

    /// Takes everything waiting to be destroyed.
    pub(crate) fn retire(&self) -> Vec<Arc<WaylandSurface>> {
        std::mem::take(&mut self.retiring.borrow_mut())
    }
}

impl core::fmt::Debug for Live {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Live")
            .field("surfaces", &self.surfaces.borrow().len())
            .field("retiring", &self.retiring.borrow().len())
            .field("exiting", &self.exiting.get())
            .finish_non_exhaustive()
    }
}
