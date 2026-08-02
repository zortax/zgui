//! The invalidation primitives: what still owes work, and where it has to be redrawn.
//!
//! Every structure here is pure data with no opinion about what a node, a frame or a surface is.
//! They exist so that doing minimal work is cheap to *decide*, because a frame that recomputes
//! only what changed still loses if working out what changed costs a walk of everything.
//!
//! | Type | Answers |
//! |---|---|
//! | [`Dirty`] | which kinds of work a node owes |
//! | [`DirtyCell`] | what one node owes, and what its whole subtree owes, in one atomic word |
//! | [`IntervalSet`] | which byte ranges of a buffer changed |
//! | [`DamageSet`] | which rectangles of a surface must be redrawn |
//! | [`EpochBitset`] | which indices a walk has already visited, without a clearing pass |
//!
//! # The shape they share
//!
//! Each one is a *bounded over-approximation*. [`DamageSet`] merges rectangles rather than
//! growing without limit, [`IntervalSet`] coalesces adjacent ranges, and [`DirtyCell`] summarises
//! a whole subtree in one word. Each answers "is there anything to do below here?" in constant
//! time and errs towards saying yes, because doing a little redundant work is a cost and missing
//! work is a bug.
//!
// The example builds a `DirtyCell`, whose atomic is the model checker's under the `loom`
// feature and panics unless it is built inside `loom::model`. The example is still compiled
// there, only not run.
#![cfg_attr(not(feature = "loom"), doc = "```")]
#![cfg_attr(feature = "loom", doc = "```no_run")]
//! use zgui_bits::{DamageSet, Dirty, DirtyCell};
//! use zgui_geom::{Point, Rect, Size};
//!
//! // A node's colour changed: it owes a repaint, and its ancestors must descend into it.
//! let node = DirtyCell::clean();
//! assert!(node.mark(Dirty::REPAINT));
//!
//! // Its ink goes into the frame's damage, and everything overlapping it is redrawn.
//! let mut damage = DamageSet::<4>::new();
//! damage.absorb(Rect::new(Point::new(20, 20), Size::new(80, 24)));
//! assert!(damage.intersects(Rect::new(Point::new(0, 0), Size::new(32, 32))));
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod damage_set;
pub mod dirty;
pub mod epoch_bitset;
pub mod interval_set;

mod sync;

pub use crate::damage_set::{DamageSet, MAX_DAMAGE, full_damage_forced};
pub use crate::dirty::{Dirty, DirtyCell};
pub use crate::epoch_bitset::EpochBitset;
pub use crate::interval_set::IntervalSet;
