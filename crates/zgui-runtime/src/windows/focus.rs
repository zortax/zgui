//! What hears about a window gaining and losing the keyboard.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// One callback told about every keyboard focus edge of a window.
type Callback = Rc<RefCell<dyn FnMut(bool)>>;

/// The callbacks one window tells about its keyboard focus edges.
#[derive(Default)]
pub(crate) struct FocusListeners {
    /// The callbacks, in the order they were registered, each with its number.
    callbacks: Vec<(u64, Callback)>,
    /// The number the next callback gets.
    next: u64,
}

impl FocusListeners {
    /// Registers `callback`, and answers with the number that removes it.
    fn insert(&mut self, callback: Callback) -> u64 {
        let id = self.next;
        self.next += 1;
        self.callbacks.push((id, callback));
        id
    }

    /// Removes the callback numbered `id`.
    fn remove(&mut self, id: u64) {
        self.callbacks.retain(|(held, _)| *held != id);
    }
}

/// Registers `callback` in `listeners`, and answers with the guard that keeps it registered.
pub(crate) fn listen(
    listeners: &Rc<RefCell<FocusListeners>>,
    callback: impl FnMut(bool) + 'static,
) -> FocusGuard {
    let id = listeners
        .borrow_mut()
        .insert(Rc::new(RefCell::new(callback)));
    FocusGuard {
        listeners: Rc::downgrade(listeners),
        id,
    }
}

/// Tells every callback in `listeners` that the window gained the keyboard or lost it.
///
/// The callbacks are copied out first, so a callback may register or remove another one. A
/// callback runs as a listener does: a signal it reads is read, not subscribed to.
pub(crate) fn tell(listeners: &Rc<RefCell<FocusListeners>>, focused: bool) {
    let callbacks: Vec<Callback> = listeners
        .borrow()
        .callbacks
        .iter()
        .map(|(_, callback)| Rc::clone(callback))
        .collect();
    let _zone = zgui_reactive::enter_non_reactive_zone();
    for callback in callbacks {
        if let Ok(mut callback) = callback.try_borrow_mut() {
            callback(focused);
        }
    }
}

/// What keeps a [`WindowHandle::on_focus_change`](crate::windows::WindowHandle::on_focus_change)
/// callback registered.
///
/// Dropping it removes the callback.
#[must_use = "dropping the guard removes the callback"]
pub struct FocusGuard {
    /// Where the callback is registered, while the window is alive.
    listeners: Weak<RefCell<FocusListeners>>,
    /// Which callback it is.
    id: u64,
}

impl Drop for FocusGuard {
    fn drop(&mut self) {
        if let Some(listeners) = self.listeners.upgrade() {
            listeners.borrow_mut().remove(self.id);
        }
    }
}

impl core::fmt::Debug for FocusGuard {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.debug_struct("FocusGuard").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{FocusListeners, listen, tell};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn every_callback_hears_each_edge_until_its_guard_goes() {
        let listeners = Rc::new(RefCell::new(FocusListeners::default()));
        let heard = Rc::new(RefCell::new(Vec::new()));
        let first = {
            let heard = Rc::clone(&heard);
            listen(&listeners, move |focused| {
                heard.borrow_mut().push(("a", focused))
            })
        };
        let second = {
            let heard = Rc::clone(&heard);
            listen(&listeners, move |focused| {
                heard.borrow_mut().push(("b", focused))
            })
        };
        tell(&listeners, true);
        drop(first);
        tell(&listeners, false);
        assert_eq!(*heard.borrow(), [("a", true), ("b", true), ("b", false)]);
        drop(second);
        assert!(listeners.borrow().callbacks.is_empty());
    }

    #[test]
    fn a_callback_may_register_another_while_it_runs() {
        let listeners = Rc::new(RefCell::new(FocusListeners::default()));
        let kept = Rc::new(RefCell::new(Vec::new()));
        let inner_listeners = Rc::clone(&listeners);
        let inner_kept = Rc::clone(&kept);
        let _guard = listen(&listeners, move |_| {
            inner_kept
                .borrow_mut()
                .push(listen(&inner_listeners, |_| {}));
        });
        tell(&listeners, true);
        assert_eq!(listeners.borrow().callbacks.len(), 2);
    }
}
