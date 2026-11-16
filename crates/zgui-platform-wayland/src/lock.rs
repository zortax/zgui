//! The session lock: the desktop hidden behind lock surfaces this application draws.
//!
//! The compositor answers a lock request later, with `locked` or with `finished`. Both answers
//! reach the application as a wake. A lock surface covers one output and is sized by its
//! configure, which the toolkit acknowledges on arrival. An output takes one lock surface of a
//! lock: a second one is a protocol error, so it is refused before it is asked for.

use std::cell::RefCell;
use std::sync::{Arc, Weak};

use smithay_client_toolkit::reexports::client::globals::GlobalList;
use smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{Connection, Proxy, QueueHandle};
use smithay_client_toolkit::session_lock::{
    SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
    SessionLockSurfaceConfigure,
};
use wayland_protocols::ext::session_lock::v1::client::ext_session_lock_manager_v1::ExtSessionLockManagerV1;
use zgui_platform::{PlatformError, Unsupported, WakeReason, Waker as _};

use crate::driver::WaylandState;

/// The lock manager, and the lock this application holds.
///
/// Behind a cell, because the borrowed context locks and unlocks through `&self`.
#[derive(Debug, Default)]
pub struct Locks {
    /// `ext_session_lock_manager_v1`, where the compositor offers it.
    manager: Option<SessionLockState>,
    /// The lock, from the request until it is unlocked or finished.
    held: RefCell<Option<SessionLock>>,
    /// The outputs that show a lock surface of the held lock, each with the claim of its surface.
    ///
    /// A claim ends when its lock surface is destroyed. An output whose claim has ended takes a
    /// new lock surface.
    covered: RefCell<Vec<(WlOutput, Weak<()>)>>,
}

/// The lock surface on one output, and its claim to that output.
#[derive(Debug)]
pub struct LockSurface {
    /// The toolkit's lock surface. Dropping it destroys it.
    surface: SessionLockSurface,
    /// What tells [`Locks`] that the output shows this surface. Dropped after the surface.
    _claim: Arc<()>,
}

impl LockSurface {
    /// The toolkit's lock surface.
    pub const fn surface(&self) -> &SessionLockSurface {
        &self.surface
    }
}

impl Locks {
    /// Binds the manager where the compositor advertised it.
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<WaylandState>) -> Self {
        let name = ExtSessionLockManagerV1::interface().name;
        let offered = globals
            .contents()
            .with_list(|list| list.iter().any(|global| global.interface == name));
        Self {
            manager: offered.then(|| SessionLockState::new(globals, qh)),
            held: RefCell::new(None),
            covered: RefCell::new(Vec::new()),
        }
    }

    /// Whether this compositor lets an application lock the session.
    pub const fn offered(&self) -> bool {
        self.manager.is_some()
    }

    /// Asks the compositor to lock the session.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Unsupported`] where the compositor has no lock manager, and
    /// [`PlatformError::Backend`] while a lock is held already.
    pub fn lock(&self, qh: &QueueHandle<WaylandState>) -> Result<(), PlatformError> {
        let Some(manager) = &self.manager else {
            return Err(Unsupported.into());
        };
        let mut held = self.held.borrow_mut();
        if held.is_some() {
            return Err(PlatformError::Backend(
                "this application holds a session lock already".to_owned(),
            ));
        }
        let lock = manager
            .lock(qh)
            .map_err(|error| PlatformError::Backend(format!("ext_session_lock_v1: {error}")))?;
        *held = Some(lock);
        self.covered.borrow_mut().clear();
        Ok(())
    }

    /// Unlocks the session, once the compositor has confirmed the lock.
    ///
    /// Before the confirmation this does nothing. Destroying a lock at that moment races the
    /// confirmation, and a destroy that arrives after it is a protocol error.
    pub fn unlock(&self) {
        let mut held = self.held.borrow_mut();
        if held.as_ref().is_some_and(SessionLock::is_locked)
            && let Some(lock) = held.take()
        {
            lock.unlock();
        }
    }

    /// Makes `wl` the lock surface on `output`, while a lock is held.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::SurfaceCreation`] when no lock is held, and when `output` shows a
    /// lock surface of this lock already.
    pub fn surface(
        &self,
        wl: WlSurface,
        output: &WlOutput,
        qh: &QueueHandle<WaylandState>,
    ) -> Result<LockSurface, PlatformError> {
        let held = self.held.borrow();
        let Some(lock) = held.as_ref() else {
            return Err(PlatformError::SurfaceCreation(
                "a lock surface needs the session lock this application holds".to_owned(),
            ));
        };
        let mut covered = self.covered.borrow_mut();
        covered.retain(|(_, claim)| claim.strong_count() > 0);
        if covered.iter().any(|(shown, _)| shown == output) {
            return Err(PlatformError::SurfaceCreation(
                "this output shows a lock surface already".to_owned(),
            ));
        }
        let claim = Arc::new(());
        covered.push((output.clone(), Arc::downgrade(&claim)));
        Ok(LockSurface {
            surface: lock.create_lock_surface(wl, output, qh),
            _claim: claim,
        })
    }

    /// Forgets the lock the compositor ended. Dropping it destroys it, as the protocol asks.
    fn finished(&self) {
        self.held.borrow_mut().take();
        self.covered.borrow_mut().clear();
    }
}

impl SessionLockHandler for WaylandState {
    fn locked(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _lock: SessionLock) {
        self.live.waker.wake(WakeReason::SessionLocked);
    }

    fn finished(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _lock: SessionLock) {
        self.locks.finished();
        self.live.waker.wake(WakeReason::SessionLockFinished);
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        _serial: u32,
    ) {
        if let Some(id) = self.identify(surface.wl_surface()) {
            self.sized_by_shell(id, configure.new_size);
        }
    }
}

smithay_client_toolkit::delegate_session_lock!(WaylandState);
