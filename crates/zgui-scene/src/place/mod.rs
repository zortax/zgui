//! Moving a box by writing where its coordinate system is.
//!
//! A box whose `transform` changed used to be composed again: its fragments rebuilt from the new
//! matrix, its primitives emitted again, its hit entry rewritten. None of that is what changed.
//! What changed is one matrix, in one node named after the box that established it — and every
//! primitive drawn under that node already names it by slot, so writing the matrix moves all of
//! them at once and costs the write.
//!
//! What the write does *not* do for itself is say which pixels have to be drawn again, so that is
//! what [`Scene::apply_place`] returns: the union of where the moved subtree's ink was and where it
//! now is.
//!
//! # Why a moving box needs an order band
//!
//! Draw order is assigned as primitives are pushed, from the rectangles they occupy, and the whole
//! of the rest of this crate rests on **equal order implying no overlap**. A matrix written after
//! the ordering is done breaks that for any motion that changes which things overlap: a scaling
//! node's children cover each other differently at 1.0 and at 1.4, and an animated matrix is
//! arbitrary.
//!
//! So a box that is going to be moved this way declares, before it is ordered, the whole region its
//! movement will visit — [`Travel`] — and is ordered against that rather than against where it
//! happens to start. The region is conservative and computed once, so a frame of the animation
//! costs no ordering work at all. A box that leaves the region it declared is refused the write and
//! composed again, which is what [`Counter::OrderBandEscapes`] counts; a box that declared no
//! region at all — an interactive drag, a gesture, anything with no known endpoints — is refused
//! for the same reason, and pays what a transform used to pay.

pub mod band;
pub mod ink;

use zgui_bits::DamageSet;
use zgui_geom::{DevicePx, Matrix4, Size, transformed_bounds};
use zgui_profile::{Counter, counter};

use crate::place::band::Travel;
use crate::scene::Scene;
use crate::spatial::SpatialId;

/// What one placement write did, and what it left owed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placement {
    /// The rectangles the move has to be redrawn over: where the ink was, and where it is.
    ///
    /// Empty when nothing moved, and empty when the write was refused — a refusal leaves the
    /// matrix where it was, so there is nothing to redraw *yet*, and what the caller owes instead
    /// is the composition it was refused.
    pub damage: DamageSet,
    /// Whether the matrix was written.
    ///
    /// False means the box has to be composed again to move at all. A caller that ignores this
    /// leaves the box where the previous frame put it.
    pub written: bool,
    /// Whether the box left the region its movement declared.
    ///
    /// A refusal with this set is the ordering hazard being caught; a refusal without it is a box
    /// that never declared a region, or a coordinate system that is no longer live.
    pub escaped: bool,
}

impl Placement {
    /// The answer for a box that has to be composed again.
    fn refused(escaped: bool) -> Self {
        counter::bump(Counter::PlaceWritesWithReemit);
        if escaped {
            counter::bump(Counter::OrderBandEscapes);
        }
        Self {
            damage: DamageSet::new(),
            written: false,
            escaped,
        }
    }
}

impl Scene {
    /// Declares the whole region a coordinate system's movement will visit.
    ///
    /// Read by [`Scene::apply_place`] and by nothing else. It is stated in the coordinates the
    /// *device* is in, because that is the space draw order is assigned in and the region's whole
    /// purpose is to be the rectangle the moving box is ordered against.
    ///
    /// Declared per coordinate system and kept until it is withdrawn, because the region is a
    /// property of the animation rather than of the frame: recomputing it per frame would be the
    /// per-frame ordering work this exists to avoid.
    pub fn declare_travel(&mut self, node: SpatialId, travel: Travel) {
        self.travel.declare(node, travel);
    }

    /// Withdraws a declaration, for an animation that has ended.
    ///
    /// A region left behind would go on admitting writes for a box whose movement is over, and the
    /// order it was given would go on being the order of a rectangle nothing occupies.
    pub fn withdraw_travel(&mut self, node: SpatialId) {
        self.travel.withdraw(node);
    }

    /// The region declared for a coordinate system, if one is.
    pub fn travel_of(&self, node: SpatialId) -> Option<Travel> {
        self.travel.of(node)
    }

    /// Widens what `node` declared so that it admits where `local` would put the subtree.
    ///
    /// What a caller does with a refusal. The box has to be composed again this frame either way,
    /// and composing it is what orders it afresh — so the region it will be ordered against next
    /// time is the region it has actually been seen in. A movement whose extent nobody could state
    /// in advance therefore converges on one within a cycle, instead of paying a composition on
    /// every frame for ever.
    pub fn widen_travel(&mut self, node: SpatialId, local: Matrix4) {
        let Some(subtree) = ink::under(self, node) else {
            return;
        };
        let (Some(before), Some(above)) = (self.spatial.resolve(node), self.above(node)) else {
            return;
        };
        let was = transformed_bounds(&before, subtree);
        let arrived = transformed_bounds(&local.then(&above), subtree);
        // The arrival, and one more step of the same movement past it. A region widened to exactly
        // where the box was seen is left again by the very next frame, which is a composition per
        // frame for the whole of a sweep and another one every time the sampling drifts; a region
        // widened by where the movement is *going* absorbs both. Being too large costs a little
        // batching and nothing else.
        let step = Size::new(
            DevicePx(arrived.left().0 - was.left().0),
            DevicePx(arrived.top().0 - was.top().0),
        );
        let widened = Travel::over(self.travel.of(node).map(Travel::region).into_iter().chain([
            was,
            arrived,
            arrived.translate(step),
        ]));
        self.travel.declare(node, widened);
    }

    /// The matrix mapping the space above `node` onto the device.
    fn above(&self, node: SpatialId) -> Option<Matrix4> {
        match self.spatial.get(node)?.parent {
            Some(parent) => self.spatial.resolve(parent),
            None => Some(Matrix4::IDENTITY),
        }
    }

    /// Opens the order band a moving box is ordered inside, and reports the order it starts at.
    ///
    /// Called as the emit walk enters a box that has declared a [`Travel`], and it is what makes a
    /// matrix written *after* ordering safe. The whole declared region is inserted, so the band
    /// begins above everything the movement will ever cover rather than above what the box happens
    /// to cover right now; the floor keeps the box's own contents inside the band, and
    /// [`Scene::close_place_band`] keeps everything drawn afterwards above it.
    ///
    /// `None` when nothing was declared, which is every box in a document that is not moving one.
    pub fn open_place_band(&mut self, node: SpatialId) -> Option<crate::id::DrawOrder> {
        let region = self.travel.of(node)?.region();
        if region.is_empty() {
            return None;
        }
        let base = self.order_mut().insert(region);
        self.order_mut().set_order_floor(base);
        Some(base)
    }

    /// Closes the band, so that what is drawn after it sorts above it wherever it goes.
    pub fn close_place_band(&mut self) {
        let above = self.max_order() + 1;
        self.order_mut().set_order_floor(above);
    }

    /// Moves the box that established `node` to `local`, as a write plus damage.
    ///
    /// This is what a transform change costs when it is expressible as one: one matrix written into
    /// the node the box already owns, and the union of where its subtree's ink was with where it
    /// now is. Nothing is styled, measured, composed or emitted, and every primitive already
    /// drawn under `node` or under anything below it moves with it because each of them names the
    /// node by slot rather than carrying a matrix of its own.
    ///
    /// The ink is read out of the primitives this scene is currently holding, which for a caller
    /// running before the frame is built is the frame that is on the screen. A caller that has
    /// begun a frame has emptied those arrays and will be told nothing moved.
    ///
    /// Refused — [`Placement::written`] false — in three cases, and a refusal means the box has to
    /// be composed again this frame:
    ///
    /// * `node` is no longer live;
    /// * no [`Travel`] was declared for it, which is every interactive transform, because a drag
    ///   has no endpoints to compute a region from;
    /// * the region declared does not contain where the box would end up.
    pub fn apply_place(&mut self, node: SpatialId, local: Matrix4) -> Placement {
        if !self.spatial.contains(node) {
            return Placement::refused(false);
        }
        let Some(travel) = self.travel.of(node) else {
            return Placement::refused(false);
        };
        let Some(before) = self.spatial.resolve(node) else {
            return Placement::refused(false);
        };
        // The chain above the node, so where the box *would* land can be asked before anything is
        // written — a refusal has to leave the matrix exactly as it found it.
        let Some(above) = self.above(node) else {
            return Placement::refused(false);
        };
        let after = local.then(&above);

        let Some(subtree) = ink::under(self, node) else {
            // Nothing is drawn through this coordinate system, so nothing moves when it does. The
            // write still happens: a box with no ink of its own can have descendants that acquire
            // some later, and leaving the matrix stale would draw them where the animation started.
            let written = self.spatial.place(node, local);
            if written {
                counter::bump(Counter::PlaceWritesWithoutReemit);
            }
            return Placement {
                damage: DamageSet::new(),
                written: true,
                escaped: false,
            };
        };

        let arrived = transformed_bounds(&after, subtree);
        if !travel.admits(arrived) {
            return Placement::refused(true);
        }

        if !self.spatial.place(node, local) {
            // The matrix already held. Nothing moved, so nothing is damaged, and counting this as
            // a saved re-emission would make the skip read high on every frame of an animation that
            // has stopped.
            return Placement {
                damage: DamageSet::new(),
                written: true,
                escaped: false,
            };
        }
        counter::bump(Counter::PlaceWritesWithoutReemit);

        let mut damage = DamageSet::new();
        damage.absorb(crate::place::ink::whole(transformed_bounds(
            &before, subtree,
        )));
        damage.absorb(crate::place::ink::whole(arrived));
        Placement {
            damage,
            written: true,
            escaped: false,
        }
    }
}

#[cfg(test)]
mod tests;
