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
use zgui_dom::NodeKey;
use zgui_layout::fragment::FragmentFlags;
use zgui_scene::SpatialId;

use crate::world::World;

/// The rectangle `node`'s fragments cover, in CSS pixels, or nothing if it generated none.
///
/// Resolved against the matrices of the frame that was drawn, which is what
/// [`World::placements`](crate::World) holds: what a consumer is told about a node's position has
/// to be what is on the screen, not what the frame being built is about to put there.
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

/// Whether any of `node`'s fragments clips what is inside it.
///
/// A consumer reads this to decide it may skip the children that are scrolled out of sight, so a
/// scrolling region that fails to declare it is one whose whole content is walked on every query.
pub fn clips_children(world: &World<'_>, node: NodeKey) -> bool {
    world.layout.fragments_of(node).iter().any(|key| {
        world
            .layout
            .fragment(*key)
            .is_some_and(|fragment| fragment.flags.contains(FragmentFlags::CLIPS_CHILDREN))
    })
}

/// Writes the geometry of `node` onto `into`.
pub fn apply(world: &World<'_>, node: NodeKey, into: &mut Node) {
    if let Some(bounds) = bounds_of(world, node) {
        into.set_bounds(bounds);
    }
    if clips_children(world, node) {
        into.set_clips_children();
    }
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
