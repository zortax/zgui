//! Changing a styled document, through the API that ships, plus the two negative controls.
//!
//! Every helper here is a call into the crate's own batched change API, so what the cases exercise
//! is the protocol as it is written rather than a copy of it. The two exceptions are the controls,
//! and they exist so that "this step is load-bearing" is a measurement rather than a claim: one
//! applies a change and then undoes the ancestor marking, and one consults a mask the caller
//! supplies rather than the one the document holds. Each reproduces, on demand, the failure the
//! corresponding step exists to prevent.

use stylo_dom::ElementState;
use zgui_bits::Dirty;
use zgui_dom::dirty::{propagate, walk};
use zgui_dom::{Document, EverythingMatters, NodeIndex, StyleFilter};
use zgui_interned::ClassName;

/// Replaces `index`'s classes through the shipped protocol.
pub(crate) fn set_classes(document: &Document, index: NodeIndex, names: &[ClassName]) {
    set_classes_with(document, &EverythingMatters, index, names);
}

/// The same, with a filter that can prove a change irrelevant.
pub(crate) fn set_classes_with(
    document: &Document,
    filter: &dyn StyleFilter,
    index: NodeIndex,
    names: &[ClassName],
) {
    document
        .edit(filter, |edit| edit.set_classes(index, names))
        .expect("the document is not poisoned");
}

/// Replaces `index`'s classes and then unmarks every ancestor that learned about it.
///
/// The negative control for the ancestor-marking step: the traversal descends only where something
/// says there is work, so without it the changed element is never reached and nothing restyles.
pub(crate) fn set_classes_without_marking(
    document: &Document,
    index: NodeIndex,
    names: &[ClassName],
) {
    set_classes(document, index, names);
    let mut current = document.store().core(index).parent();
    while let Some(parent) = current {
        let record = document.store().core(parent);
        record.dirty().retire_phase(Dirty::all(), Dirty::empty());
        record.dirty_children().clear();
        current = record.parent();
    }
}

/// Sets or clears interaction-state bits on `index` through the shipped protocol.
pub(crate) fn set_state(document: &Document, index: NodeIndex, state: ElementState, on: bool) {
    document
        .edit(&EverythingMatters, |edit| {
            edit.set_state(
                index,
                zgui_dom::node::element::state::from_engine(state),
                on,
            )
        })
        .expect("the document is not poisoned");
}

/// Writes `state` only if `mask` says a selector could care; reports whether it took the full path.
///
/// The negative control for dropping the cached state mask: the caller supplies the mask, so a mask
/// taken before a change to the element's identity can be used after it — which is exactly what a
/// cache that outlived the change would do.
pub(crate) fn set_state_filtered(
    document: &Document,
    index: NodeIndex,
    state: ElementState,
    mask: ElementState,
) -> bool {
    let changed = document.store().core(index).state() ^ state;
    if !changed.intersects(mask) {
        document.store().core(index).set_state(state);
        return false;
    }
    document
        .edit(&EverythingMatters, |edit| {
            edit.set_state(
                index,
                zgui_dom::node::element::state::from_engine(changed),
                state.intersects(changed),
            );
        })
        .expect("the document is not poisoned");
    true
}

/// Records that `index` owes a restyle, and that every ancestor must descend into it.
pub(crate) fn mark(document: &mut Document, index: NodeIndex) {
    propagate::mark(document.store_mut(), index, Dirty::RESTYLE);
}

/// Retires every obligation the way a frame's phase walks do, and reports what they visited.
///
/// A real walk rather than a sweep over every slot: it starts at the document node and descends only
/// by the dirty-child records, so a node whose subtree union was raised by something other than a
/// mark is never visited and never retired — which is the asymmetry the descent cases turn on.
pub(crate) fn retire(document: &mut Document) -> Vec<NodeIndex> {
    let mut visited = Vec::new();
    let root = document.document_index();
    walk::walk(document.store_mut(), root, Dirty::all(), &mut |_, node| {
        visited.push(node)
    });
    visited
}
