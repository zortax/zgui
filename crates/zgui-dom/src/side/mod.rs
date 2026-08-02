//! What every column holds.
//!
//! Everything about a node that is not [`Copy`] lives beside the node rather than inside it, in a
//! table indexed by the node's own slot number and reached from a worker through the record's
//! back-pointer. This module defines the value types; [`Columns`](crate::Columns) assembles them
//! and decides which are dense and which are sparse.
//!
//! Two of them hold less than their names suggest, for the same reason: a column is inside the
//! store, the store is shared with worker threads, and neither a reference count nor a closure may
//! cross that boundary. [`listeners`] holds registrations and no handlers; [`observed`] holds a mask
//! and the last delivered values, and no delivery channel. Each names where its missing half lives.

pub mod a11y_key;
pub mod anim;
pub mod attrs;
pub mod boxes;
pub mod custom_state;
pub mod drawing;
pub mod inline_style;
pub mod listeners;
pub mod observed;
pub mod paint_key;
pub mod place;
pub mod props;
pub mod semantics;

pub use crate::side::a11y_key::A11yKey;
pub use crate::side::anim::{AnimOverride, AnimSlot};
pub use crate::side::attrs::{Attr, AttrMap};
pub use crate::side::boxes::{BoxKey, BoxList, BoxNode};
pub use crate::side::custom_state::CustomStates;
pub use crate::side::inline_style::StyleBlock;
pub use crate::side::listeners::{Listener, ListenerId, ListenerSet};
pub use crate::side::observed::{ObservationSlots, ObservedMask};
pub use crate::side::paint_key::PaintStyleKey;
pub use crate::side::place::AnimPlacement;
pub use crate::side::props::PropMap;
pub use crate::side::semantics::SemanticsSlot;
