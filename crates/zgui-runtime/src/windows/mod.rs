//! The windows an application has, and how a view reaches the one it is in.
//!
//! ```no_run
//! use zgui_runtime::windows::{use_window, use_windows, WindowOptions};
//!
//! # fn example() {
//! // The window this code is running in.
//! let window = use_window();
//! window.set_title("Untitled — edited");
//!
//! // Another one, opened from anywhere: a listener, an effect, a callback. The view is a closure
//! // rather than a value because a window that is suspended and resumed builds it again.
//! let settings = use_windows().open(
//!     WindowOptions::new("Settings")
//!         .with_size(480.0, 600.0)
//!         .with_resizable(false),
//!     || zgui_elements::column(),
//! );
//! # let _ = settings;
//! # }
//! ```
//!
//! Both handles are safe to keep for as long as the application runs. Every operation on a window
//! that has closed does nothing, as does every operation this desktop cannot carry out — which is
//! what lets the code above be the same code on every platform.

mod drag;
mod handle;
mod options;

pub use crate::windows::handle::{WindowHandle, WindowId};
pub use crate::windows::options::WindowOptions;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_platform::PlatformCapabilities;
use zgui_reactive::{RwSignal, Signal, prelude::*};
use zgui_view::{Anchor, BuildCx, IntoView, View};

use crate::commands::{CloseCallbacks, CloseResponse, WindowCommands, WindowSpec};

/// Every window the application has.
///
/// Reached from anywhere inside a running application with [`use_windows`]. Bound to the thread the
/// windows run on.
#[derive(Clone)]
pub struct Windows {
    /// What a request goes onto.
    commands: WindowCommands,
    /// Every window that is open, in the order they were opened.
    registry: RwSignal<Vec<WindowHandle>, zgui_reactive::LocalStorage>,
    /// What this desktop can do.
    capabilities: Rc<RefCell<PlatformCapabilities>>,
}

impl Windows {
    /// The application's windows, over `commands`.
    pub(crate) fn new(commands: WindowCommands) -> Self {
        Self {
            commands,
            registry: RwSignal::new_local(Vec::new()),
            capabilities: Rc::new(RefCell::new(PlatformCapabilities::default())),
        }
    }

    /// Opens a window holding `view`.
    ///
    /// The handle comes back at once and the window appears on the next turn of the loop: opening
    /// one needs a platform context, and code that asks for a window is running inside a frame
    /// where there is none. Until it opens, every operation on the handle does nothing, which is
    /// the same thing they do after it closes.
    pub fn open<F, V>(&self, options: WindowOptions, mut view: F) -> WindowHandle
    where
        F: FnMut() -> V + 'static,
        V: IntoView,
    {
        let close = Rc::new(RefCell::new(CloseCallbacks::default()));
        let WindowOptions {
            attributes,
            runtime,
            on_close_request,
        } = options;
        if let Some(callback) = on_close_request {
            close.borrow_mut().insert(callback);
        }
        // Named before it is asked for, so that the handle this answers with is the one the window
        // is filled into when it opens rather than a second name for the same window.
        let token = self.commands.mint();
        let handle =
            WindowHandle::pending(token, self.commands.clone(), Rc::clone(&self.capabilities));
        let view = Box::new(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view().into_view().build(cx))
        });
        self.commands.open_named(
            token,
            WindowSpec::new(attributes, runtime, view)
                .with_close_callbacks(close)
                .with_handle(handle.clone()),
        );
        handle
    }

    /// Asks for the application to stop.
    ///
    /// What an application whose [`ExitPolicy`](crate::ExitPolicy) is
    /// [`Explicit`](crate::ExitPolicy::Explicit) has to call, and what a "Quit" menu item calls
    /// whatever the policy is.
    pub fn quit(&self) {
        self.commands.quit();
    }

    /// Every window that is open, as a snapshot.
    pub fn all(&self) -> Vec<WindowHandle> {
        self.registry.get_untracked()
    }

    /// Every window that is open, as something a view can be built from.
    ///
    /// Reading this in a view is what makes a window list that keeps itself up to date.
    pub fn watch(&self) -> Signal<Vec<WindowHandle>, zgui_reactive::LocalStorage> {
        self.registry.into()
    }

    /// What this desktop can do.
    pub fn capabilities(&self) -> PlatformCapabilities {
        self.capabilities.borrow().clone()
    }

    /// Where this records what the platform said it can do.
    pub(crate) fn capabilities_slot(&self) -> &Rc<RefCell<PlatformCapabilities>> {
        &self.capabilities
    }

    /// Records that a window is open, so that it is one of [`Windows::all`].
    pub(crate) fn note_opened(&self, handle: WindowHandle) {
        self.registry.update(|open| {
            open.retain(|other| other.id() != handle.id());
            open.push(handle);
        });
    }

    /// Records that a window is no longer open.
    pub(crate) fn note_closed(&self, id: WindowId) {
        self.registry
            .update(|open| open.retain(|handle| handle.id() != id));
    }
}

/// The window the calling code is running in.
///
/// Resolved through the scope, exactly as [`set_timeout`](zgui_view::set_timeout) and the other
/// free functions of the view layer are, so it works while a view is being built and from inside
/// any listener, effect or callback that runs under it.
///
/// # Panics
///
/// Panics when there is no window, which means it was called outside a running application. Use
/// [`try_use_window`] where that is a possibility rather than a mistake.
pub fn use_window() -> WindowHandle {
    try_use_window().expect("this code is not running inside a window")
}

/// The window the calling code is running in, if it is running in one.
pub fn try_use_window() -> Option<WindowHandle> {
    zgui_reactive::use_local_context::<WindowHandle>()
}

/// Every window the application has.
///
/// # Panics
///
/// Panics when called outside a running application.
pub fn use_windows() -> Windows {
    try_use_windows().expect("this code is not running inside an application")
}

/// Every window the application has, if there is one running.
pub fn try_use_windows() -> Option<Windows> {
    zgui_reactive::use_local_context::<Windows>()
}

/// Asks `callback` before a close the user asked for is carried out in this window.
///
/// Answering [`CloseResponse::Veto`] keeps the window open. Several components may each register
/// one and any of them refusing is enough; whatever refuses owes the user another way out.
///
/// The guard unregisters the callback when it is dropped, so a dialog that asks while it is open
/// stops asking when it closes. Keep it for as long as the question is worth asking.
///
/// # Panics
///
/// Panics when called outside a window.
#[must_use = "dropping the guard unregisters the callback"]
pub fn on_close_request(callback: impl FnMut() -> CloseResponse + 'static) -> CloseGuard {
    let callbacks = zgui_reactive::use_local_context::<Rc<RefCell<CloseCallbacks>>>()
        .expect("this code is not running inside a window");
    let id = callbacks.borrow_mut().insert(Box::new(callback));
    CloseGuard { callbacks, id }
}

/// What keeps an [`on_close_request`] callback registered.
pub struct CloseGuard {
    /// Where the callback is registered.
    callbacks: Rc<RefCell<CloseCallbacks>>,
    /// Which callback it is.
    id: u64,
}

impl Drop for CloseGuard {
    fn drop(&mut self) {
        self.callbacks.borrow_mut().remove(self.id);
    }
}

impl core::fmt::Debug for CloseGuard {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.debug_struct("CloseGuard").finish_non_exhaustive()
    }
}
