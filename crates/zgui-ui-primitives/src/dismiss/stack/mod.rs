//! Which open surface a press or an escape belongs to.

mod entry;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::Owner;

use crate::dismiss::stack::entry::Entry;

/// One registered layer's name.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct LayerId(u64);

impl LayerId {
    /// Wraps a stack's own number.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The open dismissable surfaces of one window, innermost last.
///
/// A press past an open popover inside an open dialog has to dismiss **the popover**, and only the
/// popover. Getting that wrong in either direction is a bug users notice immediately: dismissing
/// both closes the dialog they were working in, and dismissing the dialog leaves an orphaned menu
/// floating over nothing.
///
/// So exactly one layer answers, and which one is decided here — by the overlay band first and by
/// the order they opened second. The band comes first because a toast raised before a dialog is
/// still above it, and mount order alone would say otherwise.
///
/// # A layer that is on its way out
///
/// Closing is not instant: a surface that has been dismissed stays mounted until its exit
/// animation has finished, which is a tenth of a second in which it is still registered here. It
/// keeps answering **presses**, because it is still on the screen and a press has a place — one
/// that lands on a surface which is still visible must not reach the surface behind it. It stops
/// answering **Escape**, because Escape has no place: it belongs to whatever is open, and a
/// surface that has already been told to close is not. Without that distinction a dialog opened
/// from a dialog cannot be closed with two presses of Escape, because the inner one eats the
/// second while it fades.
///
/// # A layer that is no longer there at all
///
/// A layer whose element has left the document answers nothing. Its entry should have been taken
/// off by the component's own cleanup, and if that never ran — a surface that failed to unmount —
/// what is left is an entry that would otherwise swallow every press and every Escape in the
/// window for the rest of the session.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui_primitives::dismiss::LayerStack;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let stack = LayerStack::current();
///     let dialog = stack.push(OverlayLayer::Modal, NodeRef::new());
///     let popover = stack.push(OverlayLayer::Popover, NodeRef::new());
///
///     // The popover went up second but sits on a lower band, and the band decides.
///     assert_eq!(stack.topmost(), Some(dialog));
///     assert!(!stack.is_topmost(popover));
///
///     // Once the dialog has been asked to close, Escape belongs to the popover instead — while
///     // a press still belongs to the dialog, which is still on the screen.
///     stack.set_leaving(dialog, true);
///     assert_eq!(stack.answering_escape(), Some(popover));
///     assert_eq!(stack.topmost(), Some(dialog));
/// });
/// scope.unmount();
/// ```
#[derive(Clone)]
pub struct LayerStack {
    /// The open layers, in registration order.
    entries: Rc<RefCell<Vec<Entry>>>,
    /// The next number to hand out, for both names and tie-breaks.
    next: Rc<std::cell::Cell<u64>>,
}

impl LayerStack {
    /// An empty stack.
    pub fn new() -> Self {
        Self {
            entries: Rc::new(RefCell::new(Vec::new())),
            next: Rc::new(std::cell::Cell::new(1)),
        }
    }

    /// This window's stack, created and published on first use.
    ///
    /// Behind a context rather than a global, for the reason everything per-window is: two windows
    /// in one process each have their own open surfaces, and a press in one must not dismiss
    /// anything in the other.
    ///
    /// The first one created is published in the window's **root** scope rather than in whichever
    /// surface happened to ask first. That is what makes a popover and a dialog written side by
    /// side share one stack: published where the asker stands, the second one would find nothing,
    /// mint a stack of its own, and believe itself topmost — and one Escape would close both.
    pub fn current() -> Self {
        match use_local_context::<Self>() {
            Some(stack) => stack,
            None => {
                let stack = Self::new();
                match root_scope() {
                    Some(root) => root.with(|| provide_local_context(stack.clone())),
                    None => provide_local_context(stack.clone()),
                }
                stack
            }
        }
    }

    /// Registers a layer on `band` over `surface`, and reports what it is called.
    ///
    /// The handle is the layer's own element. It is what the stack asks whether the layer is still
    /// on the screen, so a layer registered over a handle that is never bound is one nothing can
    /// ever retire.
    pub fn push(&self, band: OverlayLayer, surface: NodeRef) -> LayerId {
        let ordinal = self.next.get();
        self.next.set(ordinal + 1);
        let id = LayerId::new(ordinal);
        self.entries
            .borrow_mut()
            .push(Entry::new(id, band, ordinal, surface));
        id
    }

    /// Takes a layer out. Removing one that is not there does nothing.
    pub fn pop(&self, id: LayerId) {
        self.entries.borrow_mut().retain(|entry| entry.id() != id);
    }

    /// Records whether a layer has been asked to close and is playing its exit.
    ///
    /// Setting one that is not there does nothing, which is what makes this safe to call from an
    /// effect that outlives the entry it is about.
    pub fn set_leaving(&self, id: LayerId, leaving: bool) {
        if let Some(entry) = self
            .entries
            .borrow_mut()
            .iter_mut()
            .find(|entry| entry.id() == id)
        {
            entry.set_leaving(leaving);
        }
    }

    /// The layer a press belongs to, when anything is on the screen.
    pub fn topmost(&self) -> Option<LayerId> {
        self.answering(|_| true)
    }

    /// Whether `id` is the layer a press belongs to.
    pub fn is_topmost(&self, id: LayerId) -> bool {
        self.topmost() == Some(id)
    }

    /// The layer an Escape belongs to, which is the innermost one that is still open.
    pub fn answering_escape(&self) -> Option<LayerId> {
        self.answering(|entry| !entry.leaving())
    }

    /// Whether `id` is the layer an Escape belongs to.
    pub fn answers_escape(&self, id: LayerId) -> bool {
        self.answering_escape() == Some(id)
    }

    /// The innermost live layer `want` accepts.
    ///
    /// Taken through a mutable borrow because asking is also what records that a layer has been
    /// seen on the screen: a handle that is bound now and unbound later is the only evidence there
    /// is that a surface has gone.
    fn answering(&self, want: impl Fn(&Entry) -> bool) -> Option<LayerId> {
        let mut entries = self.entries.borrow_mut();
        let mut best: Option<((OverlayLayer, u64), LayerId)> = None;
        for entry in entries.iter_mut() {
            if !entry.live() || !want(entry) {
                continue;
            }
            if best.is_none_or(|(rank, _)| entry.rank() > rank) {
                best = Some((entry.rank(), entry.id()));
            }
        }
        best.map(|(_, id)| id)
    }

    /// How many layers are open.
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    /// Whether none are.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every entry as one string, innermost last, for a trace line.
    pub(crate) fn describe(&self) -> String {
        let mut entries = self.entries.borrow_mut();
        let described: Vec<String> = entries.iter_mut().map(Entry::describe).collect();
        format!("[{}]", described.join(","))
    }
}

impl Default for LayerStack {
    fn default() -> Self {
        Self::new()
    }
}

/// The outermost scope above the calling one, which for anything inside a window is the window's.
///
/// A window's own scope has no parent — it is what the runtime creates for that window and
/// nothing else — so walking up from wherever a surface was written reaches it and stops there.
fn root_scope() -> Option<Owner> {
    let mut scope = Owner::current()?;
    while let Some(parent) = scope.parent() {
        scope = parent;
    }
    Some(scope)
}

#[cfg(test)]
mod tests {
    use zgui::prelude::{NodeRef, OverlayLayer};

    use super::LayerStack;

    #[test]
    fn within_one_band_the_last_one_open_is_the_one_that_answers() {
        let stack = LayerStack::new();
        let first = stack.push(OverlayLayer::Popover, NodeRef::new());
        let second = stack.push(OverlayLayer::Popover, NodeRef::new());
        assert_eq!(stack.topmost(), Some(second));

        // Closing the inner one hands the answer back to the outer one rather than to nothing.
        stack.pop(second);
        assert_eq!(stack.topmost(), Some(first));
    }

    #[test]
    fn the_band_beats_the_order_they_opened_in() {
        // A toast raised before a dialog paints above it, so a press belongs to the toast. Mount
        // order alone would say the dialog, which is the bug this ordering exists to prevent.
        let stack = LayerStack::new();
        let toast = stack.push(OverlayLayer::Toast, NodeRef::new());
        stack.push(OverlayLayer::Modal, NodeRef::new());
        assert_eq!(stack.topmost(), Some(toast));
    }

    #[test]
    fn a_popover_inside_a_dialog_is_the_one_that_answers() {
        // The case the whole stack exists for, written the way a user meets it: a dialog is open,
        // a menu inside it is open, and one press past the menu closes the menu.
        let stack = LayerStack::new();
        let dialog = stack.push(OverlayLayer::Modal, NodeRef::new());
        let menu = stack.push(OverlayLayer::Modal, NodeRef::new());
        assert_eq!(stack.topmost(), Some(menu));
        stack.pop(menu);
        assert_eq!(
            stack.topmost(),
            Some(dialog),
            "and the dialog is still open"
        );
    }

    #[test]
    fn an_empty_stack_answers_nothing_rather_than_guessing() {
        let stack = LayerStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.topmost(), None);
        let id = stack.push(OverlayLayer::Popover, NodeRef::new());
        stack.pop(id);
        assert_eq!(stack.topmost(), None);
        assert!(!stack.is_topmost(id));
    }

    #[test]
    fn escape_goes_to_the_innermost_surface_that_has_not_been_closed_yet() {
        // The nested case, at the moment it goes wrong: the inner surface has been dismissed and
        // is playing its exit, and the second Escape has to reach the dialog behind it rather than
        // be eaten again by the one that is already leaving.
        let stack = LayerStack::new();
        let dialog = stack.push(OverlayLayer::Modal, NodeRef::new());
        let inner = stack.push(OverlayLayer::Modal, NodeRef::new());
        assert!(stack.answers_escape(inner));

        stack.set_leaving(inner, true);
        assert!(
            stack.answers_escape(dialog),
            "the Escape belongs to the dialog the moment the inner surface has been told to close"
        );
        assert!(
            !stack.answers_escape(inner),
            "and the surface on its way out claims no more of them"
        );
        assert!(
            stack.is_topmost(inner),
            "while a press still belongs to it, because it is still on the screen"
        );
    }

    #[test]
    fn a_layer_that_was_never_built_still_answers() {
        // Registered in a component's body, one flush before the view binds its element. A stack
        // that took an unbound handle for a departed surface would drop the first key or press
        // after every open.
        let stack = LayerStack::new();
        let id = stack.push(OverlayLayer::Modal, NodeRef::new());
        assert_eq!(stack.topmost(), Some(id));
        assert_eq!(stack.answering_escape(), Some(id));
    }
}
