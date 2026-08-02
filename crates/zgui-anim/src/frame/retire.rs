//! Undoing the last frame's animation, so that this frame's is the only one in force.
//!
//! Two obligations, retired in two different ways, and the difference is not arbitrary. The
//! animating bit belongs to the frame it was marked in and is cleared wholesale, because the tick
//! that follows re-marks everything that is still running. An override belongs to the *element* and
//! survives until its animation stops, so it is dropped one element at a time, by name.

use zgui_bits::Dirty;
use zgui_dom::dirty::{propagate, walk};
use zgui_dom::{Document, NodeIndex, NodeKey};

/// Clears the animating obligation from the whole document.
///
/// Called at the start of every tick. What is still running is marked again immediately afterwards,
/// so the only elements this leaves clear are the ones that stopped — which is the point: an
/// animation that ended and left its bit behind is a loop that never parks again.
pub fn animating(document: &mut Document) {
    let root = document.document_index();
    walk::walk(
        document.store_mut(),
        root,
        Dirty::ANIMATING,
        &mut |_store, _node| {},
    );
}

/// Drops one element's cheap-path override and asks for the frame that draws it without.
pub fn override_on(document: &mut Document, index: NodeIndex) {
    let Some(node) = document.store().try_core(index).map(|_| index) else {
        return;
    };
    let key = document.store().key_of(node);
    document.store_mut().columns_mut().anim.clear(key);
    // The value on the screen is the last one the animation wrote, and the style it is drawn from
    // has changed to something else. Without this the element keeps the animation's final frame.
    propagate::mark(document.store_mut(), index, Dirty::REPAINT | Dirty::REHIT);
}

/// Asks for the frame that composes one element's box against its own style again.
///
/// Called for an element whose animation was moving it and has stopped. The placement itself is
/// dropped by the caller, which rebuilds the whole table each tick; what cannot be left to the
/// caller is the obligation, because nothing else in the frame knows the element's geometry is
/// about to change. Without it the box keeps the position the animation's last frame put it in —
/// not for a frame, but for the rest of the document's life, because the style it is composed from
/// never moved and nothing will ever compose it again.
pub fn placement_on(document: &mut Document, node: NodeKey) {
    let Some(index) = document.store().index_of(node) else {
        return;
    };
    propagate::mark(
        document.store_mut(),
        index,
        Dirty::REFRAGMENT | Dirty::REHIT,
    );
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_dom::dirty::propagate;
    use zgui_dom::side::AnimOverride;
    use zgui_dom::{Document, NodeKind};
    use zgui_interned::ElementName;

    use super::{animating, override_on, placement_on};

    /// A document with one element in it, and that element's slot.
    fn one_element() -> (Document, zgui_dom::NodeIndex) {
        let mut document = Document::new();
        let index = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("box"),
        );
        (document, index)
    }

    #[test]
    fn the_animating_bit_does_not_survive_the_tick_that_retires_it() {
        let (mut document, index) = one_element();
        propagate::mark(document.store_mut(), index, Dirty::ANIMATING);
        animating(&mut document);
        let dirty = document.store().core(index).dirty();
        assert!(!(dirty.own() | dirty.subtree()).contains(Dirty::ANIMATING));
    }

    #[test]
    fn retiring_the_bit_leaves_the_rest_of_the_obligations_alone() {
        // It is retired by name, not by clearing the word: a repaint the same element owes for an
        // unrelated reason has to survive.
        let (mut document, index) = one_element();
        propagate::mark(
            document.store_mut(),
            index,
            Dirty::ANIMATING | Dirty::RELAYOUT,
        );
        animating(&mut document);
        assert!(
            document
                .store()
                .core(index)
                .dirty()
                .own()
                .contains(Dirty::RELAYOUT)
        );
    }

    #[test]
    fn dropping_an_override_asks_for_the_frame_that_draws_without_it() {
        let (mut document, index) = one_element();
        let key = document.store().key_of(index);
        *document.store_mut().columns_mut().anim.get_mut(key) = Some(Box::new(AnimOverride {
            opacity: Some(0.5),
            ..AnimOverride::new()
        }));
        override_on(&mut document, index);
        assert!(
            document
                .store()
                .columns()
                .anim
                .get(key)
                .and_then(|slot| slot.as_ref())
                .is_none()
        );
        assert!(
            document
                .store()
                .core(index)
                .dirty()
                .own()
                .contains(Dirty::REPAINT)
        );
    }

    #[test]
    fn dropping_a_placement_asks_for_the_frame_that_composes_the_box_again() {
        // What separates this from an override being dropped: nothing about the box's style moved,
        // so a repaint would draw it again exactly where the animation left it. Only the pass that
        // composes fragments can put it back.
        let (mut document, index) = one_element();
        let key = document.store().key_of(index);
        placement_on(&mut document, key);
        let dirty = document.store().core(index).dirty();
        assert!(dirty.own().contains(Dirty::REFRAGMENT));
        assert!(dirty.own().contains(Dirty::REHIT));
    }

    #[test]
    fn dropping_the_placement_of_an_element_that_is_gone_is_not_a_panic() {
        // An element removed in the same frame its animation ended: the table still names it and
        // the name no longer resolves. Stood in for by a name from another document, which is the
        // same thing to the lookup and the only one a test can produce on purpose.
        let (mut document, _) = one_element();
        let (elsewhere, other) = one_element();
        placement_on(&mut document, elsewhere.store().key_of(other));
        assert!(
            document
                .store()
                .core(document.document_index())
                .dirty()
                .own()
                .is_empty()
        );
    }
}
