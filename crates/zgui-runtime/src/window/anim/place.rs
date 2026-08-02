//! Moving an animated box by writing the coordinate system it established.
//!
//! What a transform used to cost was the box being composed again: its fragments rebuilt from the
//! new matrix, its primitives emitted again, its hit entry rewritten. What it costs here is one
//! matrix written into a node the box already owns, plus the union of where its ink was and where
//! it is — because the fragments below it are in their own untransformed space, the hit entries
//! under them are indexed in that space, and every primitive already drawn names the node by slot.
//!
//! # Where the region it is ordered against comes from
//!
//! Draw order is assigned before anything moves, so a box that is going to be moved this way has to
//! be ordered against everywhere it will go rather than against where it starts. The display list
//! is told that region and refuses a write that would leave it.
//!
//! Nothing upstream states the region. What the tick is handed is the sampled placement for *this*
//! frame and no keyframe behind it, so the region is learnt instead of computed: a refusal widens
//! it to admit where the box was going, the box is composed again for that frame — which is what
//! orders it afresh against the widened region — and every later visit to the same place is a
//! write. A cyclic animation therefore pays a composition for the frames of its first pass and
//! writes for every pass after it.

use zgui_anim::{Placed, Placer};
use zgui_bits::DamageSet;
use zgui_dom::NodeKey;
use zgui_dom::side::AnimPlacement;
use zgui_layout::tree::store::LayoutStore;
use zgui_scene::{PropertyOwner, Scene, SpatialId};

/// Moves boxes by writing the display list's coordinate systems.
pub(crate) struct Writing<'a> {
    /// The display list holding the coordinate systems and the frame that is on the screen.
    pub scene: &'a mut Scene,
    /// The boxes, for the border box a transform is resolved against.
    pub layout: &'a LayoutStore,
    /// How many device pixels one CSS pixel is.
    pub scale: f32,
    /// Where every box moved from and to, which is what the frame has to draw again.
    ///
    /// A write moves the pixels and says nothing about which of them changed, so this is the other
    /// half of it: without the union of the two rectangles the box is drawn in its new place over a
    /// surface that still holds it in the old one.
    pub damage: DamageSet,
}

impl Placer for Writing<'_> {
    fn place(&mut self, node: NodeKey, placement: &AnimPlacement) -> Placed {
        let Some((space, matrix)) = self.moved_to(node, placement) else {
            return Placed::Recomposed;
        };
        let placement = self.scene.apply_place(space, matrix);
        if placement.written {
            self.damage.absorb_set(&placement.damage);
            self.reach(node);
            return Placed::Written;
        }
        // Refused: either nothing had been declared for this coordinate system yet, or the box was
        // heading out of what had been. Either way the frame composes it, and composing it is what
        // orders it against the region it is about to be seen in.
        self.scene.widen_travel(space, matrix);
        Placed::Recomposed
    }

    fn retired(&mut self, node: NodeKey) {
        self.withdraw(node);
    }
}

impl Writing<'_> {
    /// Keeps the box reachable by the walk that re-emits it.
    ///
    /// The emit walk decides whether to descend into a box by intersecting the damage with the
    /// device rectangle its fragments recorded — and that rectangle is written by the pass that
    /// composes them, which is the pass a write exists to avoid running. So it says where the box
    /// was when it was last composed, and a set holding only where the box came from and where it
    /// went stops reaching it after the first write. The recorded rectangle goes into the set for
    /// that reason and no other: nothing is redrawn there that was not already right.
    fn reach(&mut self, node: NodeKey) {
        let Some(box_key) = self.layout.boxes_of(node).first() else {
            return;
        };
        for frag in self.layout.fragments_of_box(*box_key) {
            if let Some(fragment) = self.layout.fragment(*frag) {
                self.damage.absorb(whole(fragment.subtree_ink));
            }
        }
    }

    /// Drops what an ended animation declared, so a region nothing is moving through stops being
    /// ordered against.
    fn withdraw(&mut self, node: NodeKey) {
        if let Some(box_key) = self.layout.boxes_of(node).first()
            && let Some(space) = self.scene.spatial.of(PropertyOwner::of(*box_key))
        {
            self.scene.withdraw_travel(space);
        }
    }

    /// The coordinate system an element's box established, and the matrix the placement puts it at.
    ///
    /// `None` for an element with no box, no fragment, no coordinate system of its own, or a
    /// placement that resolves to no transform at all — every one of which is a box the write has
    /// nowhere to go for.
    fn moved_to(&self, node: NodeKey, placement: &AnimPlacement) -> Option<(SpatialId, Matrix)> {
        let box_key = *self.layout.boxes_of(node).first()?;
        let frag = *self.layout.fragments_of_box(box_key).first()?;
        let fragment = self.layout.fragment(frag)?;
        // The same call the fragment pass makes, against the same border box, so the matrix written
        // here and the matrix a composition would arrive at are one value rather than two readings.
        let matrix = zgui_layout::fragment::transform::matrix_of(
            placement.group(),
            fragment.border_box,
            self.scale,
        )?;
        let space = self.scene.spatial.of(PropertyOwner::of(box_key))?;
        Some((space, matrix))
    }
}

/// The matrix a placement resolves to.
type Matrix = zgui_geom::Matrix4;

/// A fractional rectangle grown to the whole device pixels that can show any of it.
fn whole(
    bounds: zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>,
) -> zgui_geom::Rect<i32, zgui_geom::Device> {
    zgui_geom::Rect::from_corners(
        zgui_geom::Point::new(
            bounds.left().0.floor() as i32 - 1,
            bounds.top().0.floor() as i32 - 1,
        ),
        zgui_geom::Point::new(
            bounds.right().0.ceil() as i32 + 1,
            bounds.bottom().0.ceil() as i32 + 1,
        ),
    )
}
