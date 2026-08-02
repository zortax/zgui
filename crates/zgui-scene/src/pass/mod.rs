//! Where vector content is rasterised, and where each batch of it is composited back in.
//!
//! Vector content cannot be drawn into the frame target directly: a path rasteriser writes into a
//! scratch texture of its own and one ordinary draw composites the result back down. Getting the
//! z-order right therefore means putting that composite at exactly the right point in the stream of
//! primitives — and deciding *how many* such points there are, because each one costs a whole
//! rasterisation pass.
//!
//! **That decision lives here, not behind the rasteriser.** It is a pure function of the display
//! list, the bounds tree and the damage set, so a pass count is an assertion about the scene and is
//! checkable with no device. Behind the rasteriser it would be a number only the real renderer could
//! produce, and a test asserting it was zero would pass under a renderer that drew nothing.
//!
//! # The rules, in order
//!
//! 0. An item carrying a clip the vector scene cannot express — a sampled raster mask — ends the
//!    current pass and gets one of its own, bound to its own clip. This is the only case where a
//!    clip costs a pass.
//! 1. Drop every item whose ink misses the damage set. This is the **only** damage cull; a
//!    rasteriser must not perform another, or two owners can disagree about what survived.
//! 2. Sweep the survivors in emission order, accumulating into the current pass, keeping the pass's
//!    clip as the deepest chain its items share and each item's residual as the rest.
//! 3. Start a new pass when a non-vector primitive emitted *after* some already-accumulated item
//!    overlaps *that item's own ink*.
//! 4. End a pass where the target does: a group boundary starts a new one, because a composite is
//!    recorded into whichever target is open where it lands.
//! 5. A finished pass whose one composite cannot be placed both above every item of it and below
//!    everything painted over any of them is recorded as one pass per item instead.
//!
//! Both adjectives in rule 3 are load-bearing, and [`Overlap`] keeps the weaker readings alive as
//! selectable policy so the difference between them stays a measurement rather than a claim.
//!
//! Rule 5 is what rule 3 cannot reach. Rule 3 is consulted only when the next item arrives, so a
//! primitive painted after a pass's final item is recorded and never tested against it; and the
//! composite belongs at the **highest** order in the pass rather than the last one admitted, because
//! draw order is allocated from what a primitive overlaps and so does not rise with emission order.
//! Both facts are only decidable once the pass is complete, which is where rule 5 applies them.

pub mod coalesce;
pub mod overlap;
pub mod plan;
pub mod region;
pub(crate) mod trap;
pub mod warning;

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;

pub use crate::pass::overlap::{Intervening, Overlap};
pub use crate::pass::plan::{PlannedItem, PlannedPass, ScenePassPlan};
pub use crate::pass::warning::PassWarning;
