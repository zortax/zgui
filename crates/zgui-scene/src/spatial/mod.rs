//! Coordinate systems, named by the boxes that establish them.
//!
//! A matrix in a content-interning table is named by its *value*: interning the same matrix twice
//! returns the same identifier, and animating one therefore hands out a different identifier on
//! every frame of the animation. The nodes here are named by *structure* instead — by
//! the box that establishes the coordinate system — so an element being moved has the same name for
//! its space on the first frame of the movement and on the last, and moving it becomes a write
//! rather than a new identity.
//!
//! # Where the deduplication comes from
//!
//! Interning a matrix bought exactly one thing: a thousand identical rows shared one entry.
//! Structure buys the same thing without comparing anything, because a box that establishes no
//! coordinate system of its own is drawn in the one above it and takes its name for it. The
//! thousand rows do not each intern an equal matrix; they each name the node their parent named.
//! [`SpatialTree::space_of`] is where that happens and [`OwnSpace::of`] is what decides it.
//!
//! # What a name survives, and what it must not
//!
//! Being relaid out, restyled and having its fragments rebuilt: the box is still the box, so the
//! node is still the node. What a name must *not* survive is the box going away. A name carries an
//! occupancy counter, the slot moves on to its next counter when the frame that released it ends,
//! and a name from before compares unequal to the name after — which matters because comparing
//! names is how anything holding output cached under one decides whether that output may be reused.
//! An interned identifier was safe under that comparison because its identity *was* its value; a
//! structural name is not, and the counter is what makes it safe again.
//!
//! ```
//! use zgui_arena::DomainId;
//! use zgui_geom::Matrix4;
//! use zgui_scene::{Anchoring, OwnSpace, PropertyOwner, SpatialTree};
//!
//! let mut tree = SpatialTree::new(DomainId::FIRST);
//! let owner = |raw| PropertyOwner::new(raw).expect("a handle is never the empty word");
//! let root = tree.root(owner(1));
//!
//! let card = tree.space_of(root, owner(2), OwnSpace::of(Some(Matrix4::scale(2.0, 2.0, 1.0)), None, false));
//! let label = tree.space_of(card, owner(3), OwnSpace::of(None, None, false));
//! assert_eq!(label, card, "the label establishes nothing, so it is drawn in the card's space");
//!
//! let anchoring = tree.get(card).map(|node| node.anchoring);
//! assert_eq!(anchoring, Some(Anchoring::Scrolling));
//! ```

pub mod id;
pub mod node;
pub mod placements;
pub mod space;
pub mod tree;

#[cfg(test)]
mod tests;

pub use crate::spatial::id::{PropertyId, PropertyOwner, SPATIAL_DOMAIN, SpatialId};
pub use crate::spatial::node::{Anchoring, OwnSpace, SpatialNode};
pub use crate::spatial::placements::Placements;
pub use crate::spatial::space::SpatialTree;
pub use crate::spatial::tree::{PropertyNode, PropertyTree};
