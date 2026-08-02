//! Which element is under a point.
//!
//! The spatial part of that question is answered by the layout engine, whose index is built by the
//! same pass that writes the fragments and carries the same painting order the display list was
//! emitted in. What is left — and what is here — is the part that is about *elements* rather than
//! about boxes: the fragment under a point belongs to a box, a box belongs to an element or to no
//! element at all, and the path an event travels is the element's ancestors and not the box's.
//!
//! Those two answers differ, and a component library depends on the difference. A row whose
//! `display: contents` makes its cells children of the table generates no box of its own, so the
//! box path from a cell never passes through it — while `tr:hover td` must still match, because
//! the *element* path does.

pub mod chain;
pub mod scrollbar;

use zgui_dom::{DocumentStore, NodeKey};
use zgui_geom::{Device, DevicePx, Point};
use zgui_layout::{FragKey, HitIndex, LayoutStore};
use zgui_scene::{ClipTable, SpatialTree};

pub use crate::hit::chain::HitChain;
pub use crate::hit::scrollbar::Press as ScrollbarPress;

/// What was under a point.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// The topmost fragment under the point that resolved to an element.
    pub fragment: FragKey,
    /// The element that fragment belongs to.
    pub node: NodeKey,
    /// That element and every ancestor of it, root first.
    pub chain: HitChain,
}

/// What is under `point`, as an element and its path to the root.
///
/// Answers with nothing when the point is over no fragment, or over only fragments that belong to
/// no element and sit under no element either — an empty document's backdrop.
///
/// Fragments are considered topmost first, so a hit on an element that has been painted over
/// resolves to whatever is on top of it. A fragment that belongs to no element does not end the
/// search and does not answer with nothing either: an anonymous box is part of whatever element
/// contains it, and a press on the gap between two lines of a paragraph reaches the paragraph.
pub fn at(
    document: &DocumentStore,
    layout: &LayoutStore,
    index: &HitIndex,
    clips: &ClipTable,
    spatial: &SpatialTree,
    point: Point<DevicePx, Device>,
) -> Option<Hit> {
    for fragment in index.hit(point, clips, spatial) {
        let Some(node) = element_of(layout, fragment) else {
            continue;
        };
        let chain = HitChain::to_root(document, node);
        if chain.is_empty() {
            continue;
        }
        return Some(Hit {
            fragment,
            node,
            chain,
        });
    }
    None
}

/// The element a fragment belongs to, following box parents through anonymous boxes.
///
/// A fragment records its element directly when it has one. When it does not — an anonymous
/// wrapper, a line box, a generated marker — the answer is the nearest box above it that does,
/// which is the element a person would say they had clicked on.
pub fn element_of(layout: &LayoutStore, fragment: FragKey) -> Option<NodeKey> {
    let fragment = layout.fragment(fragment)?;
    if let Some(node) = fragment.node {
        return Some(node);
    }
    let mut box_ = Some(fragment.box_);
    while let Some(key) = box_ {
        let node = layout.get(key)?;
        if let Some(source) = node.source {
            return Some(source);
        }
        box_ = node.parent;
    }
    None
}
