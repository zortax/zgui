//! The keyboard and the pointer, limited to a set of windows.
//!
//! ```no_run
//! use zgui_runtime::grab::FocusGrab;
//! use zgui_runtime::windows::use_window;
//!
//! # fn example() {
//! // A pop-over that closes when the user presses anywhere outside it.
//! let popover = use_window();
//! if let Some(grab) = FocusGrab::new(&[&popover]) {
//!     // Where the desktop cannot limit focus, `active` stays false. Close on focus loss there.
//!     let closing = popover.clone();
//!     let _guard = grab.on_cleared(move || closing.set_visible(false));
//!     # let _ = grab;
//! }
//! # }
//! ```

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use zgui_platform::{FocusGrabId, SurfaceId};
use zgui_reactive::prelude::*;
use zgui_reactive::{ArcRwSignal, Signal};

use crate::commands::{WindowCommands, WindowStatus, WindowToken};
use crate::windows::WindowHandle;

/// One callback told that the desktop cleared a grab.
type Callback = Rc<RefCell<dyn FnMut()>>;

/// The keyboard and the pointer, limited to a set of windows until the user presses outside them.
///
/// While the grab is active, the keyboard goes to a window of the set, and follows the pointer
/// between the windows of the set. A press on any other surface clears the grab, a surface of this
/// application included. The desktop can also clear it for a reason of its own. After a clear the
/// grab has no windows, and [`FocusGrab::add`] starts it again.
///
/// Only some desktops offer a grab; see
/// [`PlatformCapabilities::focus_grab`](zgui_platform::PlatformCapabilities::focus_grab). On a
/// desktop without it the grab exists and does nothing: [`FocusGrab::active`] stays false and no
/// clear is reported. Dropping the grab stops it.
pub struct FocusGrab {
    /// What the runtime reads.
    entry: Rc<GrabEntry>,
    /// Where a change is announced.
    commands: WindowCommands,
}

/// What the runtime knows about one grab.
pub(crate) struct GrabEntry {
    /// The windows of the set, in the order they were added.
    windows: RefCell<Vec<WindowToken>>,
    /// The grab the platform made for this one, once it has made one.
    platform: Cell<Option<FocusGrabId>>,
    /// The surfaces the platform was last given.
    synced: RefCell<Vec<SurfaceId>>,
    /// Whether the grab limits focus now.
    active: ArcRwSignal<bool>,
    /// The callbacks told about a clear.
    cleared: Rc<RefCell<ClearedListeners>>,
    /// Whether the application has dropped the grab.
    dropped: Cell<bool>,
}

/// The callbacks one grab tells about a clear.
#[derive(Default)]
struct ClearedListeners {
    /// The callbacks, in the order they were registered, each with its number.
    callbacks: Vec<(u64, Callback)>,
    /// The number the next callback gets.
    next: u64,
}

impl FocusGrab {
    /// A grab over `windows`.
    ///
    /// Nothing outside a running application. The grab starts on the next turn of the loop, and a
    /// window of the set that is not open yet joins it when it opens.
    ///
    /// The desktop's capabilities are read when the grab starts, so a grab made during the
    /// application's setup works too. Where the desktop cannot limit focus,
    /// [`FocusGrab::active`] stays false.
    pub fn new(windows: &[&WindowHandle]) -> Option<Self> {
        let api = crate::windows::try_use_windows()?;
        let entry = Rc::new(GrabEntry {
            windows: RefCell::new(windows.iter().map(|window| window.id()).collect()),
            platform: Cell::new(None),
            synced: RefCell::new(Vec::new()),
            active: ArcRwSignal::new(false),
            cleared: Rc::default(),
            dropped: Cell::new(false),
        });
        let commands = api.commands().clone();
        commands.register_grab(Rc::clone(&entry));
        Some(Self { entry, commands })
    }

    /// Adds `window` to the set. Adding a window that is in the set does nothing.
    pub fn add(&self, window: &WindowHandle) {
        let mut windows = self.entry.windows.borrow_mut();
        if !windows.contains(&window.id()) {
            windows.push(window.id());
            drop(windows);
            self.commands.sync_grabs();
        }
    }

    /// Removes `window` from the set. Removing the last window stops the grab.
    pub fn remove(&self, window: &WindowHandle) {
        let mut windows = self.entry.windows.borrow_mut();
        let before = windows.len();
        windows.retain(|held| *held != window.id());
        if windows.len() != before {
            drop(windows);
            self.commands.sync_grabs();
        }
    }

    /// The windows of the set.
    pub fn windows(&self) -> Vec<crate::windows::WindowId> {
        self.entry.windows.borrow().clone()
    }

    /// Whether the grab limits focus now.
    ///
    /// True once the desktop has been given a window of the set that is open. False after a
    /// clear, while the set holds no open window, and on a desktop that cannot limit focus.
    pub fn active(&self) -> Signal<bool> {
        self.entry.active.clone().into()
    }

    /// Calls `callback` each time the desktop clears the grab.
    ///
    /// A pop-over closes itself from this. The callback runs outside every frame, as a listener
    /// does, and a signal it reads is not subscribed to. The guard removes the callback when it is
    /// dropped.
    pub fn on_cleared(&self, callback: impl FnMut() + 'static) -> ClearedGuard {
        let mut listeners = self.entry.cleared.borrow_mut();
        let id = listeners.next;
        listeners.next += 1;
        listeners
            .callbacks
            .push((id, Rc::new(RefCell::new(callback))));
        ClearedGuard {
            listeners: Rc::downgrade(&self.entry.cleared),
            id,
        }
    }
}

impl Drop for FocusGrab {
    fn drop(&mut self) {
        self.entry.dropped.set(true);
        self.commands.sync_grabs();
    }
}

impl core::fmt::Debug for FocusGrab {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("FocusGrab")
            .field("windows", &self.entry.windows.borrow())
            .field("active", &self.entry.active.get_untracked())
            .finish_non_exhaustive()
    }
}

impl GrabEntry {
    /// Gives the platform the surfaces of the windows of the set that are open, and makes or
    /// destroys the platform's grab as needed. Answers whether the grab is still wanted.
    pub(crate) fn sync(
        &self,
        cx: &dyn zgui_platform::PlatformCx,
        commands: &WindowCommands,
    ) -> bool {
        if self.dropped.get() {
            if let Some(grab) = self.platform.take() {
                cx.destroy_focus_grab(grab);
            }
            return false;
        }
        let surfaces: Vec<SurfaceId> = self
            .windows
            .borrow()
            .iter()
            .filter_map(|token| match commands.status(*token) {
                WindowStatus::Open(surface) => Some(surface),
                _ => None,
            })
            .collect();
        if *self.synced.borrow() == surfaces {
            return true;
        }
        let grab = match self.platform.get() {
            Some(grab) => grab,
            None if surfaces.is_empty() => return true,
            None => match cx.create_focus_grab() {
                Some(grab) => {
                    self.platform.set(Some(grab));
                    grab
                }
                None => return true,
            },
        };
        cx.set_focus_grab(grab, &surfaces);
        let active = !surfaces.is_empty();
        *self.synced.borrow_mut() = surfaces;
        if self.active.get_untracked() != active {
            self.active.set(active);
        }
        true
    }

    /// Whether this is the grab the platform knows as `grab`.
    pub(crate) fn is(&self, grab: FocusGrabId) -> bool {
        self.platform.get() == Some(grab)
    }

    /// Records that the desktop cleared the grab, and tells every callback.
    ///
    /// The set is empty afterwards, as it is on the desktop.
    pub(crate) fn cleared(&self) {
        self.windows.borrow_mut().clear();
        self.synced.borrow_mut().clear();
        if self.active.get_untracked() {
            self.active.set(false);
        }
        let callbacks: Vec<Callback> = self
            .cleared
            .borrow()
            .callbacks
            .iter()
            .map(|(_, callback)| Rc::clone(callback))
            .collect();
        let _zone = zgui_reactive::enter_non_reactive_zone();
        for callback in callbacks {
            if let Ok(mut callback) = callback.try_borrow_mut() {
                callback();
            }
        }
    }
}

/// What keeps a [`FocusGrab::on_cleared`] callback registered.
///
/// Dropping it removes the callback.
#[must_use = "dropping the guard removes the callback"]
pub struct ClearedGuard {
    /// Where the callback is registered, while the grab is alive.
    listeners: Weak<RefCell<ClearedListeners>>,
    /// Which callback it is.
    id: u64,
}

impl Drop for ClearedGuard {
    fn drop(&mut self) {
        if let Some(listeners) = self.listeners.upgrade() {
            listeners
                .borrow_mut()
                .callbacks
                .retain(|(held, _)| *held != self.id);
        }
    }
}

impl core::fmt::Debug for ClearedGuard {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ClearedGuard")
            .finish_non_exhaustive()
    }
}
