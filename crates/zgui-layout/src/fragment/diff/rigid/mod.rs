//! Moving a subtree that only moved.
//!
//! Layout often moves a subtree and changes nothing in it: the rows after a row that grew, or the
//! children of a sticky box whose offset changed, or the rows of a scroll container that keeps its
//! children in its own space (see [`scroll_nodes`](crate::fragment::scroll_nodes)). Every piece of
//! it is the same piece, the same size and the same shape, one offset further along. Composing it
//! again arrives at exactly that answer after reading each box's style five times, resolving its
//! sticky offset, re-deriving its ink and comparing the result against what was already there.
//!
//! So the geometry is offset instead. What that is allowed to skip is the whole of what
//! [`build::place`](crate::fragment::build::place) computes; what it is *not* allowed to skip is
//! anything downstream: the damage the movement raises, the hit entries that say where the pieces
//! now are, and the accessibility marks a moved control owes. Those are done here, in the terms the
//! comparing walk states them in, so that a frame produced this way and a frame produced by
//! composing every box put the same pixels on the screen.
//!
//! A scroll of a container that draws its children in a content space writes that space and
//! reaches none of this.
//!
//! # What makes a subtree eligible
//!
//! Three conditions, and every one of them is checked before a single fragment is touched.
//!
//! *Nothing in it owes work*, and its layout result is the one its standing fragments were composed
//! from — the test [`Pass::can_skip`](super::Pass::can_skip) already states, because leaving a
//! subtree alone and moving it rigidly rest on the same claim about it.
//!
//! *Its whole subtree is rigid*: no sticky box, whose shift is measured against a scrollport it
//! does not travel with; no box positioned against the viewport, which takes none of the scroll
//! offsets above it; and no transform, whose matrix is composed against a border box that moved.
//! That answer is folded up the fragment tree, so testing it costs one bool.
//!
//! *The origin it is snapped against did not move.* Device-pixel snapping rounds cumulative
//! absolute edges, so a subtree whose ancestors' unrounded positions changed is not one pixel
//! further along — some of its edges round the other way. Scroll and sticky offsets are added after
//! that rounding and are the only thing this path lets differ, which is why the offset need not be
//! a whole number of pixels for the result to be exact.
//!
//! # Clips
//!
//! A clipping box's rectangle travels with the box, so its descendants are drawn under the same
//! chain of the same shapes measured somewhere else. The chain is issued from the moved padding box
//! as the walk passes through the box that imposes it, which is the same call the composing walk
//! makes and therefore arrives at the same identifier.
//!
//! The identifier is the one the box already had, and that is the point rather than a convenience.
//! A chain is named by its rectangle with the accumulated scroll and sticky offsets taken back out,
//! so the walk carries those offsets down beside the chain: the name does not move when the box
//! does, and a glide of a thousand frames issues the chains it started with instead of a fresh set
//! per frame.
//!
//! # Two duties, and how they are told apart
//!
//! The moving and the telling are independent of each other, and one descent does both only
//! because they want the same recursion. [`duty`] is how a descent is asked for one of them, and
//! [`split`](super::split) is what asks. A frame still discharges both, in the same order relative
//! to the boxes they act on; it makes several descents rather than one, and each is timed.

mod duty;
mod walk;

use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Size};
use zgui_scene::ClipId;

use crate::fragment::FragmentFlags;
use crate::fragment::diff::dirty::FrameDirty;
use crate::fragment::diff::split::{self, Part, Passes};
use crate::fragment::diff::{Folded, Pass};

use self::walk::Walk;

/// How far a subtree moved, and under what it is now drawn.
#[derive(Clone, Copy, Debug)]
pub(super) struct Move {
    /// The offset every piece of it takes.
    pub(super) by: (f32, f32),
    /// How far the scroll and sticky offsets above the subtree moved: all of `by` for a change of
    /// shift, and the part of a layout move that a scroll in the same frame added.
    ///
    /// Every box the move carries adds it to the shift its contents were composed against. A box
    /// left with the old shift reads the same scroll as a movement again the next time it is
    /// composed, and offsets its clean contents twice.
    pub(super) scrolled: (f32, f32),
    /// The chain the subtree's own root is drawn under, after the move.
    pub(super) clip: ClipId,
    /// Everything the scroll and sticky offsets above the subtree add up to, after the move.
    ///
    /// It is what the chains the subtree issues are named by, and it is carried rather than
    /// re-derived because the walk that issues them composes nothing and so resolves no offsets of
    /// its own.
    pub(super) shift: (f32, f32),
}

impl Move {
    /// The same offset as a size, which is what a rectangle is translated by.
    fn size(self) -> Size<DevicePx, Device> {
        Size::new(DevicePx(self.by.0), DevicePx(self.by.1))
    }

    /// The accumulated shift as a size.
    fn shift(self) -> Size<DevicePx, Device> {
        Size::new(DevicePx(self.shift.0), DevicePx(self.shift.1))
    }
}

impl<D: FrameDirty> Pass<'_, '_, D> {
    /// Whether a clean child can be moved rather than composed again.
    ///
    /// Read beside [`Pass::can_skip`](super::Pass::can_skip), which the caller has already asked:
    /// this is the extra claim moving needs over leaving alone, and it is entirely about the
    /// subtree's own styles.
    pub(super) fn can_translate(&self, child: BoxKey) -> bool {
        self.store
            .fragments_of_box(child)
            .first()
            .and_then(|frag| self.store.fragment(*frag))
            .is_some_and(|fragment| fragment.subtree_rigid)
    }

    /// Offsets one clean subtree, and reports what it folds up exactly as visiting it would have.
    ///
    /// The damage is raised once for the whole subtree rather than once per piece. The two cover
    /// the same pixels — a subtree's ink *is* the union of its pieces' ink, which is what
    /// [`Fragment::subtree_ink`](crate::Fragment::subtree_ink) holds — and the union is what a
    /// bounded damage set converges to after absorbing the pieces one at a time anyway.
    pub(super) fn translate(&mut self, key: BoxKey, moved: Move) -> Folded {
        let Some(&root) = self.store.fragments_of_box(key).first() else {
            return Folded::empty();
        };
        let Some(fragment) = self.store.fragment(root) else {
            return Folded::empty();
        };
        let before = fragment.subtree_ink;
        let anchor = fragment.anchor;
        let folded = Folded {
            subtree_ink: before.translate(moved.size()),
            blending: fragment
                .flags
                .contains(FragmentFlags::HAS_BLENDING_DESCENDANT),
            reads: fragment.flags.intersects(FragmentFlags::READS_BACKDROP),
            disjoint: fragment.subtree_disjoint,
            rigid: true,
            order: self.store.subtree_order(key),
            anchor: Some(anchor),
            bound: fragment.flags.intersection(FragmentFlags::BOUND),
        };
        // The frame cannot answer this by moving pixels: what was under the subtree is now
        // something else, and so is what is under where it went. Both land beyond any rigid move
        // the frame reports, so a caller that moves the pixels of a content space in the same
        // frame still draws them.
        let admitted = self.admitted(moved.clip);
        let was = self.device_before(before, anchor);
        self.damage_beyond_a_move(was, super::Admitted::everything());
        let is = self.device_now(folded.subtree_ink, anchor);
        self.damage_beyond_a_move(is, admitted);
        // Inside a content space that moved, the frame may move the port's pixels by the space's
        // vector, and the subtree's old pixels go with them. For a row that layout moved in the
        // frame the port scrolled, no box above damages that rectangle.
        if let Some(by) = self.chase.by {
            let carried = was.translate(Size::new(DevicePx(by.0), DevicePx(by.1)));
            self.damage_beyond_a_move(carried, admitted);
        }

        let scale = self.tables.device.scale;
        let mut walk = Walk::over(self.store, self.hit, self.tables, self.dirty, moved, scale);
        match self.passes {
            Passes::Together => walk.subtree::<duty::Both>(key, moved.clip, moved.shift()),
            Passes::TogetherTimed => {
                split::timed(Part::Together, || {
                    walk.subtree::<duty::Both>(key, moved.clip, moved.shift())
                });
            }
            // The order is the fused walk's own: a box's rectangles move before anything is told
            // where they went, and the two traversals that precede both touch nothing at all.
            // Doing the index descent first would index the pieces where they were.
            Passes::Apart => {
                split::timed(Part::Skeleton, || {
                    walk.subtree::<duty::Skeleton>(key, moved.clip, moved.shift());
                });
                split::timed(Part::Warmed, || {
                    walk.subtree::<duty::Skeleton>(key, moved.clip, moved.shift());
                });
                split::timed(Part::Geometry, || {
                    walk.subtree::<duty::Geometry>(key, moved.clip, moved.shift());
                });
                split::timed(Part::Index, || {
                    walk.subtree::<duty::Index>(key, moved.clip, moved.shift())
                });
            }
        }
        if self.passes != Passes::Together {
            split::walked();
        }
        folded
    }
}
