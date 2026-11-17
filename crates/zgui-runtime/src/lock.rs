//! The session lock: the desktop hidden behind lock windows this application draws.
//!
//! ```no_run
//! use zgui_reactive::RenderEffect;
//! use zgui_reactive::prelude::*;
//! use zgui_runtime::lock::SessionLock;
//! use zgui_runtime::monitors::use_monitors;
//!
//! # fn example() -> Result<(), zgui_runtime::lock::LockError> {
//! let lock = SessionLock::lock()?;
//!
//! // One lock window per output, including the outputs connected while the session is locked.
//! // An output that has a lock window keeps it.
//! let monitors = use_monitors();
//! let opener = lock.clone();
//! let effect = RenderEffect::new(move |_| {
//!     for id in monitors.get().iter().filter_map(|monitor| monitor.id) {
//!         opener.open_surface(id, zgui_elements::column);
//!     }
//! });
//!
//! // Later, once the user has authenticated.
//! if lock.locked().get_untracked() {
//!     lock.unlock();
//! }
//! # drop(effect);
//! # Ok(())
//! # }
//! ```

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use zgui_platform::{MonitorId, SurfaceAttributes};
use zgui_reactive::prelude::*;
use zgui_reactive::{ArcRwSignal, Signal};
use zgui_view::IntoView;

use crate::commands::{WindowCommands, WindowStatus};
use crate::windows::{WindowHandle, WindowOptions, Windows};

/// Why a session lock could not be asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LockError {
    /// No application is running on this thread.
    #[error("no application is running on this thread")]
    NoApplication,
    /// This application holds a session lock already.
    #[error("this application holds a session lock already")]
    Held,
}

/// Locks the session.
///
/// The desktop hides every window and shows the lock windows of the application that holds the
/// lock. Only some desktops let an application lock the session; see
/// [`PlatformCapabilities::session_lock`](zgui_platform::PlatformCapabilities::session_lock). A
/// desktop without it ends the lock at once, and [`LockHandle::finished`] turns true.
#[derive(Debug)]
pub struct SessionLock;

impl SessionLock {
    /// Asks the desktop to lock the session.
    ///
    /// The request goes out on the next turn of the loop. [`LockHandle::locked`] turns true when
    /// the desktop confirms the lock, and [`LockHandle::finished`] turns true when it refuses the
    /// lock or ends it. Open a lock window for each output with [`LockHandle::open_surface`] at
    /// once: a desktop confirms the lock after every output shows a lock window.
    ///
    /// # Errors
    ///
    /// Returns [`LockError::NoApplication`] outside a running application, and
    /// [`LockError::Held`] while an earlier lock is asked for or held.
    pub fn lock() -> Result<LockHandle, LockError> {
        let windows = crate::windows::try_use_windows().ok_or(LockError::NoApplication)?;
        let commands = windows.commands().clone();
        if commands.session_lock().is_some() {
            return Err(LockError::Held);
        }
        let entry = Rc::new(LockEntry {
            phase: Cell::new(Phase::Asked),
            locked: ArcRwSignal::new(false),
            finished: ArcRwSignal::new(false),
            windows: RefCell::new(Vec::new()),
        });
        commands.request_lock(Rc::clone(&entry));
        Ok(LockHandle {
            entry,
            windows,
            commands,
        })
    }
}

/// A session lock this application asked for.
///
/// Cheap to clone. Dropping every clone keeps the session locked: only [`LockHandle::unlock`]
/// unlocks it.
#[derive(Clone)]
pub struct LockHandle {
    /// What the runtime reads.
    entry: Rc<LockEntry>,
    /// Where a lock window is opened.
    windows: Windows,
    /// Where an unlock is asked for.
    commands: WindowCommands,
}

/// Where a session lock is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Asked for, and not yet confirmed.
    Asked,
    /// Confirmed by the desktop.
    Locked,
    /// Unlocked by the application, or ended by the desktop.
    Over,
}

/// What the runtime knows about one session lock.
pub(crate) struct LockEntry {
    /// Where the lock is in its life.
    phase: Cell<Phase>,
    /// Whether the desktop confirmed the lock and it holds now.
    locked: ArcRwSignal<bool>,
    /// Whether the desktop refused the lock or ended it.
    finished: ArcRwSignal<bool>,
    /// The lock windows, in the order they were opened, each with its output.
    windows: RefCell<Vec<(MonitorId, WindowHandle)>>,
}

impl LockHandle {
    /// Opens a lock window holding `view` on the output `monitor`.
    ///
    /// The desktop gives the window the extent of the whole output, and the keyboard. Open one for
    /// each output, and one for each output connected while the session is locked; read the
    /// outputs with [`use_monitors`](crate::monitors::use_monitors). The window closes with
    /// [`CloseReason::OutputGone`](zgui_platform::CloseReason::OutputGone) when its output goes
    /// away, and it closes when the lock is over. A window opened after that does not open.
    ///
    /// An output takes one lock window. While the window for `monitor` is open or about to open,
    /// this answers that window and leaves `view` unused.
    pub fn open_surface<F, V>(&self, monitor: MonitorId, view: F) -> WindowHandle
    where
        F: FnMut() -> V + 'static,
        V: IntoView,
    {
        if let Some(held) = self.surface_on(monitor) {
            return held;
        }
        let mut options = WindowOptions::new("");
        options.attributes = SurfaceAttributes::lock(monitor);
        let handle = self.windows.open(options, view);
        self.entry
            .windows
            .borrow_mut()
            .push((monitor, handle.clone()));
        handle
    }

    /// The lock window on `monitor` that is open or about to open.
    fn surface_on(&self, monitor: MonitorId) -> Option<WindowHandle> {
        self.entry
            .windows
            .borrow()
            .iter()
            .filter(|(on, _)| *on == monitor)
            .map(|(_, window)| window)
            .find(|window| self.commands.status(window.id()) != WindowStatus::Closed)
            .cloned()
    }

    /// Whether the desktop confirmed the lock and it holds now.
    ///
    /// False before the confirmation, after [`LockHandle::unlock`] and after the desktop ended
    /// the lock.
    pub fn locked(&self) -> Signal<bool> {
        self.entry.locked.clone().into()
    }

    /// Whether the desktop refused the lock or ended it.
    ///
    /// The lock windows close when this turns true. A new lock can be asked for after it.
    pub fn finished(&self) -> Signal<bool> {
        self.entry.finished.clone().into()
    }

    /// Unlocks the session and closes the lock windows.
    ///
    /// Only a lock the desktop confirmed can be unlocked. Answers whether the unlock was asked
    /// for: false before [`LockHandle::locked`] turns true, and after the lock is over.
    ///
    /// A lock that waits for its answer cannot be cancelled. The desktop can confirm it at any
    /// moment, and a lock destroyed after the confirmation is a protocol error. Wait for
    /// [`LockHandle::locked`] or [`LockHandle::finished`], and unlock then.
    ///
    /// The session stays locked after every handle is dropped. Keep a handle to unlock with.
    pub fn unlock(&self) -> bool {
        if self.entry.phase.get() != Phase::Locked {
            return false;
        }
        self.entry.phase.set(Phase::Over);
        self.entry.locked.set(false);
        self.commands.request_unlock(Rc::clone(&self.entry));
        true
    }

    /// The lock windows that were opened, open or not.
    pub fn windows(&self) -> Vec<WindowHandle> {
        self.entry.windows()
    }
}

impl core::fmt::Debug for LockHandle {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("LockHandle")
            .field("phase", &self.entry.phase.get())
            .field("windows", &self.entry.windows.borrow().len())
            .finish_non_exhaustive()
    }
}

impl LockEntry {
    /// Whether the lock is asked for or held.
    pub(crate) fn is_live(&self) -> bool {
        self.phase.get() != Phase::Over
    }

    /// Records that the desktop confirmed the lock.
    pub(crate) fn confirmed(&self) {
        if self.phase.get() == Phase::Asked {
            self.phase.set(Phase::Locked);
            self.locked.set(true);
        }
    }

    /// Records that the desktop refused the lock or ended it.
    pub(crate) fn ended(&self) {
        self.phase.set(Phase::Over);
        if self.locked.get_untracked() {
            self.locked.set(false);
        }
        self.finished.set(true);
    }

    /// The lock windows.
    pub(crate) fn windows(&self) -> Vec<WindowHandle> {
        self.windows
            .borrow()
            .iter()
            .map(|(_, window)| window.clone())
            .collect()
    }
}
