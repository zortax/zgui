//! The order the document is painted in, as a walk with an inside and an outside.
//!
//! Painting order is not document order and it is not the box tree's order either. A document is
//! painted as a forest of stacking contexts, and inside each one the children are painted in
//! Appendix E's passes: negative stacking children, then block-level backgrounds, then floats, then
//! inline content, then positioned and zero-index children, then positive stacking children.
//!
//! Which pass a box belongs to, whether it establishes a context and what it sorts by inside its
//! pass are all the layout stage's answers, called from here. What this adds is the shape the
//! emission needs and a flat list cannot give: an *enter* and a *leave* for each box, so a group can
//! be opened before its subtree and closed after it, and an outline can be drawn after the
//! descendants it must sit over.

use std::borrow::Cow;

use smallvec::SmallVec;
use zgui_dom::side::BoxKey;
use zgui_layout::LayoutStore;
use zgui_layout::fragment::stacking::{level, z_index};

/// What a walk does at each box.
pub trait Visitor {
    /// Called before a box is ranked among its siblings. Returning `false` drops its subtree.
    ///
    /// This is the cheap question, asked once per child of every entered box, and it exists
    /// because ranking is the expensive part of order: which pass a child paints in only matters
    /// for a child that paints this frame, and a damaged corner of a long list must not rank a
    /// thousand rows to find the two it touches.
    fn descends(&mut self, _store: &LayoutStore, _key: BoxKey) -> bool {
        true
    }
    /// Called on the way in. Returning `false` skips the box's subtree entirely.
    fn enter(&mut self, store: &LayoutStore, key: BoxKey) -> bool;
    /// Called on the way out, and only for a box whose [`Visitor::enter`] returned `true`.
    fn leave(&mut self, store: &LayoutStore, key: BoxKey);
}

/// Walks `root` and everything below it in painting order.
///
/// Only the children the visitor descends into are ranked. The relative order of the emitted
/// children is the one the full sort gives, because the sort is stable and a dropped child
/// contributes nothing to interleave with.
pub fn walk(store: &LayoutStore, root: BoxKey, visitor: &mut impl Visitor) {
    if !visitor.enter(store, root) {
        return;
    }
    if let Some(node) = store.get(root) {
        let mut kept: SmallVec<[(u8, i32, usize, BoxKey); 16]> = SmallVec::new();
        for (position, &child) in node.children.iter().enumerate() {
            if visitor.descends(store, child) {
                kept.push((0, 0, position, child));
            }
        }
        for entry in &mut kept {
            entry.0 = level(store, entry.3) as u8;
            entry.1 = z_index(store, entry.3);
        }
        kept.sort_by_key(|(pass, index, position, _)| (*pass, *index, *position));
        for (_, _, _, child) in kept {
            walk(store, child, visitor);
        }
    }
    visitor.leave(store, root);
}

/// One box's children, in the order they are painted.
///
/// The sort is stable, so two children in the same pass with the same `z-index` keep the order they
/// are laid out in — which is the tie-break the specification gives, and which `order` on a flex
/// item has already moved, exactly as it moves painting.
///
/// Borrowed where the two orders already agree, which is nearly every box: a container of ordinary
/// block or inline children puts all of them in one pass at one index, and a stable sort of that is
/// the list it started with. Recognising it costs one walk and saves two allocations and the sort,
/// per entered box, per frame.
pub fn children_in_paint_order(store: &LayoutStore, key: BoxKey) -> Cow<'_, [BoxKey]> {
    let Some(node) = store.get(key) else {
        return Cow::Borrowed(&[]);
    };
    let mut ranks = node
        .children
        .iter()
        .map(|&child| (level(store, child) as u8, z_index(store, child)));
    let mut previous = ranks.next();
    let sorted = ranks.all(|rank| {
        let ordered = previous.is_some_and(|last| last <= rank);
        previous = Some(rank);
        ordered
    });
    if sorted {
        return Cow::Borrowed(&node.children);
    }

    let mut children: Vec<(u8, i32, usize, BoxKey)> = node
        .children
        .iter()
        .enumerate()
        .map(|(position, &child)| {
            (
                level(store, child) as u8,
                z_index(store, child),
                position,
                child,
            )
        })
        .collect();
    children.sort_by_key(|(pass, index, position, _)| (*pass, *index, *position));
    Cow::Owned(children.into_iter().map(|(_, _, _, child)| child).collect())
}

#[cfg(test)]
mod tests {
    use zgui_dom::side::BoxKey;
    use zgui_layout::LayoutStore;

    use super::{Visitor, walk};

    /// A minted key, for a test that needs a name and not a stored value.
    fn box_key<T>(index: u32) -> zgui_arena::Key<T> {
        zgui_arena::Key::new(
            index,
            zgui_arena::Generation::new(1).expect("one is a generation"),
            zgui_arena::DomainId::FIRST,
        )
    }

    /// A visitor that records the sequence of enters and leaves.
    #[derive(Default)]
    struct Trace {
        /// Each step, as the box and whether it was an entry.
        steps: Vec<(BoxKey, bool)>,
        /// Boxes whose subtree is to be skipped.
        skip: Vec<BoxKey>,
    }

    impl Visitor for Trace {
        fn enter(&mut self, _store: &LayoutStore, key: BoxKey) -> bool {
            self.steps.push((key, true));
            !self.skip.contains(&key)
        }

        fn leave(&mut self, _store: &LayoutStore, key: BoxKey) {
            self.steps.push((key, false));
        }
    }

    #[test]
    fn a_box_whose_subtree_is_skipped_is_never_left() {
        // The pairing matters: a group opened on the way in and closed on the way out would be left
        // open by a skip that still reported a leave, or closed twice by one that reported two.
        let store = LayoutStore::new(zgui_dom::Document::new().store().document());
        let missing = box_key(7);
        let mut trace = Trace {
            skip: vec![missing],
            ..Trace::default()
        };
        walk(&store, missing, &mut trace);
        assert_eq!(trace.steps, vec![(missing, true)]);
    }

    #[test]
    fn a_box_that_is_entered_is_always_left() {
        let store = LayoutStore::new(zgui_dom::Document::new().store().document());
        let key = box_key(3);
        let mut trace = Trace::default();
        walk(&store, key, &mut trace);
        assert_eq!(trace.steps, vec![(key, true), (key, false)]);
    }
}
