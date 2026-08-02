//! Interaction state, and what a view is watching.
//!
//! A pointer moving across a list writes one state bit per frame, twice — off the row it left and
//! on to the row it entered — so this is the highest-frequency change a running interface makes and
//! the one whose cost matters most. Two things keep it cheap.
//!
//! **The record a state change takes holds the state and nothing else.** The element's classes have
//! not moved, so copying them would be work with no consumer.
//!
//! **A bit no selector could match on is written and forgotten.** Nothing styles `:read-only`,
//! `:in-range` or `:indeterminate` in an ordinary component library, so writing one cannot change a
//! computed value anywhere: no record is taken, no ancestor is marked, and the style engine is not
//! entered at all. The set of bits that *can* matter is asked of the rule set per element, because
//! a document-wide answer is worthless — every real stylesheet styles `:hover` somewhere, so a
//! document-wide mask has the hover bit set and every hover anywhere takes the slow path.
//!
//! The handful of bits that reach the accessibility projection whether or not any rule mentions
//! them are the exception: skipping the style engine for those is right, and skipping the
//! accessibility update is not, so they mark that and only that.

use zgui_bits::Dirty;
use zgui_vocab::UiState;

use crate::id::node_key::NodeIndex;
use crate::mutate::ancestors;
use crate::mutate::edit::Edit;
use crate::side::observed::{ObservationSlots, ObservedMask};

/// The state bits an assistive technology reads whether or not any rule mentions them.
///
/// A change to one of these has to reach the accessibility projection even when no selector in the
/// document depends on it, because the projection reads the element's state directly rather than
/// its computed style.
const ANNOUNCED: UiState = UiState::from_bits(
    UiState::CHECKED.bits()
        | UiState::DISABLED.bits()
        | UiState::OPEN.bits()
        | UiState::INDETERMINATE.bits(),
);

impl Edit<'_> {
    /// Turns the state bits in `mask` on or off for `node`.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_state(&mut self, node: NodeIndex, mask: UiState, on: bool) {
        let bits = crate::node::element::state::to_engine(mask);
        let held = self.store().core(node).state();
        let wanted = if on { held | bits } else { held - bits };
        if wanted == held {
            return;
        }
        let changed = held ^ wanted;

        if !changed.intersects(self.watched_states(node)) {
            // Nothing that matches can have changed, so the style engine is not entered. This is
            // deliberately not a repaint either: if no selector names the bit, no computed value
            // moved, and a repaint here is the frame this path exists to avoid.
            self.store().core(node).set_state(wanted);
            if crate::node::element::state::from_engine(changed).intersects(ANNOUNCED) {
                ancestors::mark(self.store(), node, Dirty::A11Y);
            }
            return;
        }

        let (store, batch) = self.parts();
        batch.snapshots.record_state(store, node);
        store.core(node).set_state(wanted);
        ancestors::mark(store, node, Dirty::RESTYLE);
        if crate::node::element::state::from_engine(changed).intersects(ANNOUNCED) {
            ancestors::mark(store, node, Dirty::A11Y);
        }
    }

    /// Records which of `node`'s measurements something is watching.
    ///
    /// This is the one change that takes no record and marks nothing: what a view observes is not
    /// something any selector can see, so it cannot change a computed value, and the measurements
    /// themselves are produced by layout regardless of who is listening. Called by whatever holds
    /// the registry of observers, when a node gains its first watcher or loses its last.
    ///
    /// # Why the cache goes with it
    ///
    /// A measurement is delivered only when it differs from what was last handed out, and what was
    /// last handed out is remembered here rather than by the watcher. A watcher that has gone took
    /// its signal with it; the next one to arrive starts with nothing and is told only when the
    /// value *changes* — so a box that has not moved since is never delivered at all, and the new
    /// watcher waits for ever on a measurement that was taken long ago and thrown away.
    ///
    /// That is not a rare shape. It is every floating surface after the first: a popover, a menu
    /// and a tooltip each measure the window's root while they are open and give the share back
    /// when they close, so the second surface a window ever opens is placed against a viewport it
    /// was never told the size of — and is therefore never placed, and never painted.
    ///
    /// So the cache is emptied whenever the set of watchers changes. The cost is one extra
    /// delivery per registration; the alternative is a surface that silently does not appear.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_observed(&mut self, node: NodeIndex, mask: ObservedMask) {
        let store = self.store();
        let key = store.key_of(node);
        let slots = store.columns_mut().observed.get_mut(key);
        if slots.mask == mask {
            return;
        }
        *slots = ObservationSlots {
            mask,
            ..ObservationSlots::NONE
        };
    }

    /// Records the measurements that were last handed to `node`'s watchers.
    ///
    /// The mask is left exactly as it was: which measurements are watched is the registry's
    /// answer, and this is only the cache of what those watchers have already been told. Without
    /// it every frame in which a fragment moved would deliver every watched measurement again,
    /// whether or not it changed — and a value delivered again is a signal written again, which is
    /// an effect re-run and a frame.
    ///
    /// Marks nothing and records nothing, for the same reason
    /// [`Edit::set_observed`](Self::set_observed) does not: no selector can see it.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn record_observed(&mut self, node: NodeIndex, delivered: &ObservationSlots) {
        let store = self.store();
        let key = store.key_of(node);
        let slots = store.columns_mut().observed.get_mut(key);
        slots.border_box = delivered.border_box;
        slots.content_size = delivered.content_size;
        slots.scroll_offset = delivered.scroll_offset;
        slots.scrollport = delivered.scrollport;
    }
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_interned::ElementName;
    use zgui_vocab::UiState;

    use crate::arena::document::Document;
    use crate::mutate::filter::EverythingMatters;
    use crate::node::kind::NodeKind;
    use crate::side::observed::ObservedMask;

    /// A document with one element, and that element.
    fn one() -> (Document, crate::id::node_key::NodeIndex) {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        (document, root)
    }

    #[test]
    fn observing_a_node_marks_nothing_and_records_nothing() {
        let (document, root) = one();
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_observed(root, ObservedMask::BORDER_BOX);
            })
            .expect("not poisoned");

        assert_eq!(document.pending_snapshots(), 0);
        assert!(document.store().core(root).dirty().own().is_clean());
        assert_eq!(
            document
                .store()
                .columns()
                .observed
                .get(document.store().key_of(root))
                .map(|slots| slots.mask),
            Some(ObservedMask::BORDER_BOX)
        );
    }

    #[test]
    fn writing_a_state_bit_that_is_already_set_does_nothing_at_all() {
        let (document, root) = one();
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_state(root, UiState::HOVER, true);
                edit.set_state(root, UiState::HOVER, true);
            })
            .expect("not poisoned");
        assert_eq!(document.pending_snapshots(), 1);
        assert!(
            document
                .store()
                .core(root)
                .dirty()
                .own()
                .contains(Dirty::RESTYLE)
        );
    }
}
