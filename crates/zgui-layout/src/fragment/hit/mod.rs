//! Hit testing: which fragments are under a point.
//!
//! The index lives here, beside the fragments it indexes, and not with the input system that
//! queries it. Two reasons, and the first is not a preference: its bulk build reads the layout
//! store and its entries are named by fragment, while the pass that writes those entries is this
//! crate's — an index anywhere above would make the dependency go both ways. The second is that
//! hit order *is* painting order, and painting order is decided here.
//!
//! What the input system owns is the other half of the question. This index answers in fragments;
//! turning those into the element chain that capture and bubble dispatch walks is a question about
//! elements, and the two answers differ whenever an element generates boxes that are not its own —
//! a row whose `display: contents` makes its cells the children of the table.

pub mod entry;
pub mod index;
pub mod pointer_events;
mod rtree;
pub mod transform;

pub use crate::fragment::hit::entry::HitEntry;
pub use crate::fragment::hit::index::HitIndex;
pub use crate::fragment::hit::pointer_events::PointerEvents;

use zgui_geom::{Corners, Vec2};
use zgui_scene::DrawOrder;

use crate::fragment::FragKey;
use crate::tree::store::LayoutStore;

/// The index entry for one fragment, at one place in painting order.
///
/// Built from the fragment and the box behind it, so that the index and the fragment tree cannot
/// hold different opinions about where something is or whether it can be clicked.
pub fn entry_for(
    store: &LayoutStore,
    frag: FragKey,
    order: DrawOrder,
    scale: f32,
) -> Option<HitEntry> {
    let fragment = store.fragment(frag)?;
    let node = store.get(fragment.box_)?;
    // A scrollbar is a piece of the box's chrome rather than a piece of its border, so it does not
    // take the box's corner radii: a bar down the side of a card with a twelve-pixel radius would
    // otherwise refuse presses along most of its length.
    let radii = if matches!(
        fragment.kind,
        crate::fragment::FragmentKind::Scrollbar { .. }
    ) {
        Corners::uniform(Vec2::splat(zgui_geom::DevicePx(0.0)))
    } else {
        crate::fragment::clip::radii(&node.style, fragment.border_box, scale)
    };
    Some(HitEntry {
        frag,
        node: fragment.node,
        order,
        clip: fragment.clip,
        clip_space: fragment.clip_transform,
        space: fragment.transform,
        pointer_events: pointer_events::of(&node.style),
        radii,
        bounds: fragment.border_box,
        envelope: fragment.local_ink,
    })
}
