//! Measuring a custom element again because its implementation asked to be.
//!
//! A custom element reads its style and measures itself in `layout`, and nowhere else. When the
//! application changes what the element would measure — its content, a decoration that names a
//! colour the last layout never read — it moves the element's layout revision, and the document
//! owes the node a relayout. Nothing about the node's *style* moved, so the restyle pass does not
//! reach it; this pass does, and throws away the box's held answer so that the next layout asks
//! the implementation again.
//!
//! The walk follows the dirty-child records and retires nothing: the same obligation is what the
//! fragment pass later visits the box by.

use zgui_bits::Dirty;
use zgui_dom::{Document, NodeIndex};

use crate::tree::dirty::mark_dirty;
use crate::tree::store::LayoutStore;

/// Throws away the layout of every custom box whose element owes a relayout under `root`.
///
/// Returns how many boxes were invalidated, ancestors included.
pub fn relayout(store: &mut LayoutStore, document: &Document, root: NodeIndex) -> u32 {
    let source = document.store();
    let mut marked = 0;
    let mut stack = vec![root];
    while let Some(index) = stack.pop() {
        let core = source.core(index);
        let (own, subtree) = core.dirty().get();
        if !(own | subtree).intersects(Dirty::RELAYOUT) {
            continue;
        }
        if own.contains(Dirty::RELAYOUT) {
            let key = source.key_of(index);
            for box_ in store.boxes_of(key).to_vec() {
                if store.custom_content(box_).is_some() {
                    marked += mark_dirty(store, box_);
                }
            }
        }
        stack.extend(core.dirty_children().iter(source, index));
    }
    marked
}
