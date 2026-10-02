//! The callbacks an application registers for what happens outside its windows.

use std::cell::RefCell;
use std::rc::Rc;

use zgui_platform::MenuId;

/// A callback shared between the registry and a dispatch that is running it.
type Shared<F> = Rc<RefCell<F>>;

/// Every callback registered for one kind of event, by the id its guard removes it with.
struct List<F: ?Sized> {
    /// The callbacks, in registration order.
    entries: Vec<(u64, Shared<F>)>,
}

impl<F: ?Sized> Default for List<F> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<F: ?Sized> List<F> {
    /// The callbacks as they are now.
    ///
    /// A dispatch runs a copy, so a callback can register or remove a callback while it runs.
    fn snapshot(&self) -> Vec<Shared<F>> {
        self.entries.iter().map(|(_, f)| Rc::clone(f)).collect()
    }
}

/// The registry behind [`on_reopen`], [`on_menu`] and [`on_exit`].
#[derive(Default)]
struct Registry {
    /// The next id to hand out.
    next: u64,
    /// What runs when the user asks for the application again.
    reopen: List<dyn FnMut()>,
    /// What runs when the user picks a menu action.
    menu: List<dyn FnMut(&MenuId)>,
    /// What runs when the application exits.
    exit: List<dyn FnMut()>,
}

/// Which list a guard removes its callback from.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// [`on_reopen`].
    Reopen,
    /// [`on_menu`].
    Menu,
    /// [`on_exit`].
    Exit,
}

/// The application's callbacks, provided in the scope above every window.
#[derive(Clone, Default)]
pub(crate) struct AppHooks {
    /// The callbacks.
    registry: Rc<RefCell<Registry>>,
}

impl AppHooks {
    /// A registry with no callbacks.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Mints the id of the next callback.
    fn mint(&self) -> u64 {
        let mut registry = self.registry.borrow_mut();
        registry.next += 1;
        registry.next
    }

    /// Runs every reopen callback.
    pub(crate) fn reopen(&self) {
        let callbacks = self.registry.borrow().reopen.snapshot();
        for callback in callbacks {
            (callback.borrow_mut())();
        }
    }

    /// Runs every menu callback with `id`.
    pub(crate) fn menu(&self, id: &MenuId) {
        let callbacks = self.registry.borrow().menu.snapshot();
        for callback in callbacks {
            (callback.borrow_mut())(id);
        }
    }

    /// Runs every exit callback.
    pub(crate) fn exit(&self) {
        let callbacks = self.registry.borrow().exit.snapshot();
        for callback in callbacks {
            (callback.borrow_mut())();
        }
    }

    /// Registers a reopen callback.
    pub(crate) fn add_reopen(&self, callback: impl FnMut() + 'static) -> AppHookGuard {
        let id = self.mint();
        let callback: Shared<dyn FnMut()> = Rc::new(RefCell::new(callback));
        self.registry
            .borrow_mut()
            .reopen
            .entries
            .push((id, callback));
        self.guard(Kind::Reopen, id)
    }

    /// Registers a menu callback.
    pub(crate) fn add_menu(&self, callback: impl FnMut(&MenuId) + 'static) -> AppHookGuard {
        let id = self.mint();
        let callback: Shared<dyn FnMut(&MenuId)> = Rc::new(RefCell::new(callback));
        self.registry.borrow_mut().menu.entries.push((id, callback));
        self.guard(Kind::Menu, id)
    }

    /// Registers an exit callback.
    pub(crate) fn add_exit(&self, callback: impl FnMut() + 'static) -> AppHookGuard {
        let id = self.mint();
        let callback: Shared<dyn FnMut()> = Rc::new(RefCell::new(callback));
        self.registry.borrow_mut().exit.entries.push((id, callback));
        self.guard(Kind::Exit, id)
    }

    /// The guard that removes callback `id` from the list `kind` names.
    fn guard(&self, kind: Kind, id: u64) -> AppHookGuard {
        AppHookGuard {
            hooks: self.clone(),
            kind,
            id,
        }
    }

    /// Removes callback `id` from the list `kind` names.
    fn remove(&self, kind: Kind, id: u64) {
        let mut registry = self.registry.borrow_mut();
        match kind {
            Kind::Reopen => registry.reopen.entries.retain(|(own, _)| *own != id),
            Kind::Menu => registry.menu.entries.retain(|(own, _)| *own != id),
            Kind::Exit => registry.exit.entries.retain(|(own, _)| *own != id),
        }
    }
}

/// What keeps an application callback registered.
///
/// Dropping it removes the callback. Keep it for as long as the callback should run.
#[must_use = "dropping the guard removes the callback"]
pub struct AppHookGuard {
    /// Where the callback is registered.
    hooks: AppHooks,
    /// Which list holds it.
    kind: Kind,
    /// Which callback it is.
    id: u64,
}

impl Drop for AppHookGuard {
    fn drop(&mut self) {
        self.hooks.remove(self.kind, self.id);
    }
}

impl core::fmt::Debug for AppHookGuard {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("AppHookGuard")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// The registry of the running application.
fn hooks() -> AppHooks {
    zgui_reactive::use_local_context::<AppHooks>()
        .expect("this code is not running inside an application")
}

/// Runs `callback` when the user asks for the application again.
///
/// A click on the dock icon asks for it on macOS. An application that hid its last window shows
/// it again here. Desktops without such a request never call it; see
/// [`PlatformCapabilities::reopen`](zgui_platform::PlatformCapabilities::reopen).
///
/// # Panics
///
/// Panics when called outside a running application.
pub fn on_reopen(callback: impl FnMut() + 'static) -> AppHookGuard {
    hooks().add_reopen(callback)
}

/// Runs `callback` with the id of each application-menu action the user picks.
///
/// # Panics
///
/// Panics when called outside a running application.
pub fn on_menu(callback: impl FnMut(&MenuId) + 'static) -> AppHookGuard {
    hooks().add_menu(callback)
}

/// Runs `callback` once, as the application exits.
///
/// Every exit runs it: the last window closing, a quit the application asked for, and a quit the
/// desktop asked for. It runs on the UI thread before the windows close, and the process may end
/// as soon as it returns, so it finishes its work before it returns.
///
/// # Panics
///
/// Panics when called outside a running application.
pub fn on_exit(callback: impl FnMut() + 'static) -> AppHookGuard {
    hooks().add_exit(callback)
}
