//! Listeners that follow whichever element a handle is bound to.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::RenderEffect;
use zgui::view::NodeId;

use crate::diag::note;

/// Listeners kept on whichever element a [`NodeRef`] is bound to.
///
/// A handle is not an element. Content that is taken away and put back is a **new** element bound
/// to the same handle, and listeners attached the first time it bound stay on the departed one,
/// where they hear nothing at all. Anything waiting on an event from such a listener waits for
/// ever — and because the element only changes on the *second* time round, the first pass looks
/// perfect and every one after it is deaf.
///
/// So the element the listeners went on is remembered, and they are taken off it and put on the
/// new one whenever the handle binds to something else. Dropping this takes them off.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui_ui_primitives::presence::Listening;
///
/// # fn example(surface: NodeRef) {
/// let listening = Listening::new(surface, move || {
///     surface
///         .listen(events::ANIMATION_END, ListenerOptions::DEFAULT, |_| {})
///         .into_iter()
///         .collect()
/// });
/// # drop(listening);
/// # }
/// ```
#[must_use = "dropping this removes the listeners immediately"]
pub struct Listening {
    /// What puts them back when the handle rebinds.
    ///
    /// Held so that dropping this stops the re-attaching before the listeners themselves go, which
    /// is what stops a teardown putting a fresh set on the way out.
    watching: Option<RenderEffect<()>>,
    /// The listeners as they stand, which is one element's worth or none.
    guards: Rc<RefCell<Vec<ListenerGuard>>>,
}

impl Listening {
    /// Puts the listeners `attach` makes on `handle`'s element, and moves them when it rebinds.
    ///
    /// `attach` is called once per element the handle binds to, and answers with the guards it
    /// took. It is not called at all while the handle is unbound, because there is nothing to
    /// listen to.
    pub fn new<F>(handle: NodeRef, attach: F) -> Self
    where
        F: Fn() -> Vec<ListenerGuard> + 'static,
    {
        Self::named(0, handle, attach)
    }

    /// The same, under a number that names the owner in a trace.
    pub(crate) fn named<F>(who: u64, handle: NodeRef, attach: F) -> Self
    where
        F: Fn() -> Vec<ListenerGuard> + 'static,
    {
        let guards: Rc<RefCell<Vec<ListenerGuard>>> = Rc::new(RefCell::new(Vec::new()));
        let watching = {
            let guards = Rc::clone(&guards);
            // Which element they are on. Compared rather than counted, because the question is not
            // "are any attached" but "are they attached to the element the handle names now".
            let on: Rc<Cell<Option<NodeId>>> = Rc::new(Cell::new(None));
            RenderEffect::new(move |_| {
                // Read first and unconditionally: this is what brings the effect back when the
                // handle binds, unbinds and binds again.
                let node = handle.get();
                if on.get() == node {
                    note!(
                        "listen.same",
                        "who={who} node={node:?} guards={}",
                        guards.borrow().len()
                    );
                    return;
                }
                note!(
                    "listen.rebind",
                    "who={who} from={:?} to={node:?} dropping={}",
                    on.get(),
                    guards.borrow().len()
                );
                on.set(node);
                // Off the old element before anything goes on the new one. Dropping a guard for an
                // element that has already left the tree is allowed and removes the handler, which
                // is the half that does not live in the document.
                guards.borrow_mut().clear();
                if node.is_some() {
                    let held = attach();
                    note!(
                        "listen.attached",
                        "who={who} node={node:?} guards={} on={:?}",
                        held.len(),
                        held.iter().map(ListenerGuard::node).collect::<Vec<_>>()
                    );
                    *guards.borrow_mut() = held;
                }
            })
        };
        Self {
            watching: Some(watching),
            guards: Rc::clone(&guards),
        }
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        drop(self.watching.take());
        self.guards.borrow_mut().clear();
    }
}
