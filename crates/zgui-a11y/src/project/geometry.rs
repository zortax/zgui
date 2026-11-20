//! Where a node is, as an accessibility tree measures it.
//!
//! Two decisions are baked in here rather than left to each call site.
//!
//! **Bounds are the union of a node's fragments.** An element that breaks across two columns or
//! three lines is one node with one rectangle, because that is the thing a consumer draws a
//! highlight around.
//!
//! **Bounds are in CSS pixels and the root carries the scale.** Reporting device pixels would work
//! until the window moved to a display of a different scale, at which point every node in the tree
//! would have to be rewritten to say the same thing in different numbers.
//!
//! **Bounds are resolved through the coordinate system the fragment is in.** A fragment keeps its
//! rectangle in its own space, which is the space its clip, its corner radii and its children are
//! expressed in and is *not* where it is drawn as soon as anything above it carries a transform.
//! Reading the fragment's own rectangle gives a consumer the rectangle the element would occupy if
//! nothing had moved it — a highlight drawn half a panel away from the control it belongs to, and
//! an element being animated reported at the place it started from for the whole animation.

use accesskit::{Affine, Node, Rect};
use zgui_dom::{NodeKey, NodeKind};
use zgui_scene::SpatialId;

use crate::world::World;

/// Where a node is, as its accessibility node states it.
#[derive(Clone, Debug, PartialEq)]
pub struct Geometry {
    /// The node's rectangle in its own coordinate space, or nothing if it generated no box.
    pub bounds: Option<Rect>,
    /// The transform from the node's coordinate space to its anchor's, when the node carries one.
    pub transform: Option<Affine>,
}

impl Geometry {
    /// Writes both properties onto `into`, clearing whichever is absent.
    pub fn apply(&self, into: &mut Node) {
        match self.bounds {
            Some(bounds) => into.set_bounds(bounds),
            None => into.clear_bounds(),
        }
        match self.transform {
            Some(transform) => into.set_transform(transform),
            None => into.clear_transform(),
        }
    }

    /// What a node holding `held` states.
    pub fn of_node(held: &Node) -> Self {
        Self {
            bounds: held.bounds(),
            transform: held.transform().copied(),
        }
    }
}

/// The geometry `node` states: its rectangle relative to its anchor, and its own transform.
///
/// A node with element children carries a transform to its content origin — its own origin less
/// whatever it is scrolled by — and states its rectangle in that space, so the rectangle is at the
/// scroll offset and its children are measured from the unscrolled content. A leaf states its
/// rectangle relative to the nearest ancestor that carries a transform.
pub fn measure(world: &World<'_>, node: NodeKey) -> Geometry {
    let Some(absolute) = bounds_of(world, node) else {
        return Geometry {
            bounds: None,
            transform: None,
        };
    };
    let anchor = anchor_origin(world, node);
    if has_element_child(world, node) {
        let (scroll_x, scroll_y) = scroll_of(world, node);
        let transform = Affine::translate((
            absolute.x0 - scroll_x - anchor.0,
            absolute.y0 - scroll_y - anchor.1,
        ));
        let bounds = Rect::new(
            scroll_x,
            scroll_y,
            scroll_x + absolute.width(),
            scroll_y + absolute.height(),
        );
        return Geometry {
            bounds: Some(bounds),
            transform: Some(transform),
        };
    }
    Geometry {
        bounds: Some(Rect::new(
            absolute.x0 - anchor.0,
            absolute.y0 - anchor.1,
            absolute.x1 - anchor.0,
            absolute.y1 - anchor.1,
        )),
        transform: None,
    }
}

/// The origin of the coordinate space `node`'s rectangle is stated in, in CSS pixels.
///
/// The content origin of the nearest ancestor carrying a transform, or the window's for a node
/// with no such ancestor. An ancestor with element children but no box of its own — one whose
/// `display` is `contents` — carries nothing and is walked past.
fn anchor_origin(world: &World<'_>, node: NodeKey) -> (f64, f64) {
    let store = world.document.store();
    let mut current = store
        .index_of(node)
        .and_then(|index| store.core(index).parent());
    while let Some(index) = current {
        if store.core(index).kind() != NodeKind::Element {
            break;
        }
        let key = store.key_of(index);
        if has_element_child(world, key)
            && let Some(bounds) = bounds_of(world, key)
        {
            let (scroll_x, scroll_y) = scroll_of(world, key);
            return (bounds.x0 - scroll_x, bounds.y0 - scroll_y);
        }
        current = store.core(index).parent();
    }
    (0.0, 0.0)
}

/// Where on the page the box of `node` starts, in CSS pixels, or nothing if it generated no box.
///
/// Its children are measured from this origin less the scroll. A scroll carries the children
/// along with it, so only this origin tells that a child may now sit somewhere else relative to
/// its parent.
pub fn origin(world: &World<'_>, node: NodeKey) -> Option<(f64, f64)> {
    let bounds = bounds_of(world, node)?;
    Some((bounds.x0, bounds.y0))
}

/// Whether `node` has an element among its children, which is what makes it carry a transform.
fn has_element_child(world: &World<'_>, node: NodeKey) -> bool {
    let store = world.document.store();
    let Some(index) = store.index_of(node) else {
        return false;
    };
    let mut next = store.core(index).first_child();
    while let Some(child) = next {
        if store.core(child).kind() == NodeKind::Element {
            return true;
        }
        next = store.core(child).next_sibling();
    }
    false
}

/// How far `node`'s content is scrolled, in CSS pixels.
fn scroll_of(world: &World<'_>, node: NodeKey) -> (f64, f64) {
    let Some(offsets) = world.scroll else {
        return (0.0, 0.0);
    };
    let scale = f64::from(if world.scale > 0.0 { world.scale } else { 1.0 });
    let offset = offsets.of(node);
    (f64::from(offset.x.0) / scale, f64::from(offset.y.0) / scale)
}

/// The rectangle `node`'s fragments cover in the window, in CSS pixels, or nothing if it
/// generated none.
///
/// Resolved against the matrices of the frame that was drawn, which is what
/// [`World::placements`](crate::World) holds: what a consumer is told about a node's position has
/// to be what is on the screen, not what the frame being built is about to put there. What a node
/// *states* is relative to its anchor; see [`measure`].
pub fn bounds_of(world: &World<'_>, node: NodeKey) -> Option<Rect> {
    let scale = f64::from(if world.scale > 0.0 { world.scale } else { 1.0 });
    let covered = zgui_layout::fragment::transform::placed::placed_union(
        world.layout,
        world.layout.fragments_of(node),
        world.placements,
    )?;
    Some(Rect::new(
        f64::from(covered.left().0) / scale,
        f64::from(covered.top().0) / scale,
        f64::from(covered.right().0) / scale,
        f64::from(covered.bottom().0) / scale,
    ))
}

/// Every coordinate system `node`'s rectangle was measured through.
///
/// One for nearly every node, and the point of answering it at all is that the *name* of a
/// coordinate system does not change when the matrix under it does. A node filed under the names
/// its bounds depend on can be found again when one of those names resolves to something else,
/// which is the only way anything holding a published rectangle finds out it is stale.
pub fn spaces_of<'a>(world: &'a World<'_>, node: NodeKey) -> impl Iterator<Item = SpatialId> + 'a {
    world
        .layout
        .fragments_of(node)
        .iter()
        .filter_map(|key| world.layout.fragment(*key))
        .filter_map(|fragment| fragment.transform)
}

/// Writes the geometry of `node` onto `into`.
///
/// No node states that it clips its children, whatever its box does. A consumer leaves out each
/// child that a clipping node cuts off, and its walk over the children of a node then stops at
/// the first such child that has children of its own and follows an empty sibling. That takes
/// the rows a scroll moved out of a port from the reader, and the rows the port shows with them.
/// A reader tells a row out of sight from its rectangle, which is where the scroll put it.
///
/// The cost is in a consumer's hit test, which also goes into each child a clip would cut off.
/// It can answer a point outside a scroll port with a row scrolled out of that port.
pub fn apply(world: &World<'_>, node: NodeKey, into: &mut Node) {
    measure(world, node).apply(into);
}

/// The transform the root node carries.
///
/// Every other node's rectangle is measured in CSS pixels in the space this establishes, which is
/// why it is the only transform in the tree.
pub fn root_transform(scale: f32) -> Affine {
    Affine::scale(f64::from(scale))
}

#[cfg(test)]
mod tests {
    use super::root_transform;

    #[test]
    fn the_root_carries_the_scale_and_nothing_else() {
        assert_eq!(
            root_transform(2.0).as_coeffs(),
            [2.0, 0.0, 0.0, 2.0, 0.0, 0.0]
        );
    }
}
