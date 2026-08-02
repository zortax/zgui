//! Finding the pieces one element was painted as.
//!
//! One element produces several fragments — one per line of a split inline, one per column, one per
//! page — so "where is this element" is a union and not a rectangle, and every consumer that asks
//! has to be given the union rather than the first piece. This is that question, answered once.
//!
//! Not to be confused with the hit index, which is a *spatial* structure over the same fragments:
//! this one answers "which pieces belong to this element", that one answers "which pieces are under
//! this point".

use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Rect};

use crate::fragment::FragKey;
use crate::tree::store::LayoutStore;

/// The border boxes of every piece one element was painted as, unioned.
///
/// Empty when the element generated no boxes, which is what `display: none` produces and is a
/// different answer from a zero-sized box at the origin — the rectangle is empty either way, so a
/// caller that needs to tell them apart asks whether the element has fragments at all.
pub fn bounds_of(store: &LayoutStore, node: NodeKey) -> Rect<DevicePx, Device> {
    union(store, store.fragments_of(node).iter().copied())
}

/// The ink of every piece one element was painted as, unioned with everything below them.
///
/// This is what has to be redrawn when the element goes away, and it is deliberately the *subtree*
/// ink: a removed panel takes its children's pixels with it.
pub fn ink_of(store: &LayoutStore, node: NodeKey) -> Rect<DevicePx, Device> {
    let mut held: Option<Rect<DevicePx, Device>> = None;
    for frag in store.fragments_of(node) {
        let Some(fragment) = store.fragment(*frag) else {
            continue;
        };
        held = Some(match held {
            Some(union) => union.union(fragment.subtree_ink),
            None => fragment.subtree_ink,
        });
    }
    held.unwrap_or(Rect::ZERO)
}

/// The union of a set of fragments' border boxes.
fn union(store: &LayoutStore, fragments: impl Iterator<Item = FragKey>) -> Rect<DevicePx, Device> {
    let mut held: Option<Rect<DevicePx, Device>> = None;
    for frag in fragments {
        let Some(fragment) = store.fragment(frag) else {
            continue;
        };
        held = Some(match held {
            Some(union) => union.union(fragment.border_box),
            None => fragment.border_box,
        });
    }
    held.unwrap_or(Rect::ZERO)
}
