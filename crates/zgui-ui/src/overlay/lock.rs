//! Stopping the window scrolling behind a modal surface, without moving it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, Owner, RenderEffect};
use zgui::view::{HostHandle, current_host};

/// Stops the window scrolling for as long as anything holds it.
///
/// A dialog that leaves the window scrollable behind it is a dialog the user scrolls away from,
/// and a wheel gesture over a modal surface has nowhere sensible to go. So the window is locked
/// while one is open.
///
/// # What locking is, and what it is not
///
/// The window is **frozen**, not restyled. Nothing about the page changes: it keeps every scroll
/// container it had, keeps the offset each one is at, keeps the width its content wrapped to and
/// keeps the gutter its scrollbar occupies. What stops is movement, and it stops for every way in
/// — a wheel, a trackpad, a key, an accessibility action and a scroll a view asks for outright.
///
/// The obvious implementation is `:root { overflow: hidden }`, and it is wrong in a way that is
/// plainly visible: a root that is no longer a scroll container has no offset composed into its
/// descendants, so the page snaps to the top the instant a dialog opens and snaps back when it
/// closes. A scroll lock that moves the page is not a scroll lock.
///
/// It is counted rather than a flag, and that is the whole reason it is a type: a dialog opened
/// from a dialog is two holders, and the inner one closing must not unfreeze the window while the
/// outer one is still there. The count lives in the enclosing scope's context, so two windows in
/// one process lock independently.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui::overlay::ScrollLock;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let lock = ScrollLock::current();
///     assert_eq!(lock.holders(), 0);
///
///     let outer = lock.acquire();
///     let inner = lock.acquire();
///     assert_eq!(lock.holders(), 2);
///
///     drop(inner);
///     assert_eq!(lock.holders(), 1, "the outer dialog is still open");
///     drop(outer);
///     assert_eq!(lock.holders(), 0);
/// });
/// scope.unmount();
/// ```
#[derive(Clone)]
pub struct ScrollLock {
    /// How many surfaces are holding it.
    holders: Rc<Cell<usize>>,
    /// The engine the freeze is asked of, when there is a window to ask.
    ///
    /// Taken once, where the lock is created, rather than at each acquire: a hold is taken and
    /// given back from a reactive effect, and the scope an effect re-runs in is not the scope the
    /// component was built in.
    host: Option<HostHandle>,
}

impl ScrollLock {
    /// A lock nothing is holding, over the window the calling scope is in.
    #[must_use]
    pub fn new() -> Self {
        Self {
            holders: Rc::new(Cell::new(0)),
            host: current_host(),
        }
    }

    /// This window's lock, created and published on first use.
    ///
    /// The first one created is published in the window's **root** scope rather than in whichever
    /// surface happened to ask first. That is what makes a dialog and a sheet written side by side
    /// share one count: published where the asker stands, the second one would find nothing and
    /// mint a lock of its own, and because both freeze the same window, whichever closed first
    /// would thaw it while the other was still open — leaving the window scrolling behind a modal
    /// surface.
    #[must_use]
    pub fn current() -> Self {
        match use_local_context::<Self>() {
            Some(lock) => lock,
            None => {
                let lock = Self::new();
                match root_scope() {
                    Some(root) => {
                        let published = lock.clone();
                        root.with(move || provide_local_context(published));
                    }
                    None => provide_local_context(lock.clone()),
                }
                lock
            }
        }
    }

    /// How many surfaces are holding it.
    #[must_use]
    pub fn holders(&self) -> usize {
        self.holders.get()
    }

    /// Takes a hold on it. Dropping the guard gives it back.
    #[must_use = "dropping the guard releases the lock immediately"]
    pub fn acquire(&self) -> ScrollLockGuard {
        self.holders.set(self.holders.get() + 1);
        if self.holders.get() == 1 {
            self.freeze(true);
        }
        ScrollLockGuard {
            lock: Some(self.clone()),
        }
    }

    /// Gives one hold back.
    fn release(&self) {
        let held = self.holders.get().saturating_sub(1);
        self.holders.set(held);
        if held == 0 {
            self.freeze(false);
        }
    }

    /// Tells the window whether it may move.
    ///
    /// Nothing happens outside a window, which is what a lock exercised on its own in a test is.
    fn freeze(&self, frozen: bool) {
        if let Some(host) = &self.host {
            host.freeze_scrolling(frozen);
        }
    }
}

impl Default for ScrollLock {
    fn default() -> Self {
        Self::new()
    }
}

/// One hold on a [`ScrollLock`], released when it is dropped.
#[must_use = "dropping the guard releases the lock immediately"]
pub struct ScrollLockGuard {
    /// The lock, taken on the way out so a guard releases exactly once.
    lock: Option<ScrollLock>,
}

impl Drop for ScrollLockGuard {
    fn drop(&mut self) {
        if let Some(lock) = self.lock.take() {
            lock.release();
        }
    }
}

/// Holds the window's [`ScrollLock`] for as long as `active` reads true.
///
/// What every modal surface calls, once, from its own body. The hold is given back when the
/// calling scope goes away, so a dialog that is unmounted rather than closed does not leave the
/// window frozen for ever.
pub fn use_scroll_lock(active: Signal<bool, LocalStorage>) {
    let lock = ScrollLock::current();
    let held: Rc<RefCell<Option<ScrollLockGuard>>> = Rc::new(RefCell::new(None));
    let watching = {
        let held = Rc::clone(&held);
        RenderEffect::new(move |_| {
            if active.get() {
                if held.borrow().is_none() {
                    *held.borrow_mut() = Some(lock.acquire());
                }
            } else {
                held.borrow_mut().take();
            }
        })
    };
    on_cleanup_local(move || {
        drop(watching);
        held.borrow_mut().take();
    });
}

/// The outermost scope above the calling one, which for anything inside a window is the window's.
///
/// A window's own scope has no parent — it is what the runtime creates for that window and nothing
/// else — so walking up from wherever a surface was written reaches it and stops there.
fn root_scope() -> Option<Owner> {
    let mut scope = Owner::current()?;
    while let Some(parent) = scope.parent() {
        scope = parent;
    }
    Some(scope)
}

#[cfg(test)]
mod tests {
    use zgui::reactive::{Mounted, install};

    use super::ScrollLock;

    #[test]
    fn a_lock_is_per_scope_rather_than_global() {
        // Two windows in one process each scroll on their own, and a dialog in one must not
        // freeze the other.
        install().ok();
        let first = Mounted::new();
        let second = Mounted::new();
        let held = first.with(|| {
            let lock = ScrollLock::current();
            let guard = lock.acquire();
            core::mem::forget(guard);
            lock.holders()
        });
        assert_eq!(held, 1);
        assert_eq!(second.with(|| ScrollLock::current().holders()), 0);
        first.unmount();
        second.unmount();
    }
}
