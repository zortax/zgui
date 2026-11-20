//! The coordinate systems, clips and side tables that layout, input and painting resolve geometry
//! through.
//!
//! A [`SpatialTree`] names each coordinate system after the box that establishes it, and
//! [`Placements`] caches the matrix of every name for one frame. A [`ClipTable`] stores clip
//! chains as links that each name the coordinate system they are drawn in. Clip chains and other
//! interned values live in a [`Table`], a persistent map with stable ids and a journal of the
//! slots that changed.
//!
//! ```
//! use zgui_arena::DomainId;
//! use zgui_geom::{Device, DevicePx, Point, Rect, Size};
//! use zgui_space::{ClipLink, ClipTable, OwnSpace, PropertyOwner, SpatialTree};
//!
//! let mut tree = SpatialTree::new(DomainId::FIRST);
//! let owner = |raw| PropertyOwner::new(raw).expect("a handle is never the empty word");
//! let root = tree.root(owner(1));
//! let card = tree.space_of(root, owner(2), OwnSpace::of(None, None, false));
//! assert_eq!(card, root, "a box with no transform is drawn in its parent's space");
//!
//! let mut clips = ClipTable::rooted();
//! let rect: Rect<DevicePx, Device> =
//!     Rect::new(Point::new(DevicePx(0.0), DevicePx(0.0)), Size::new(DevicePx(64.0), DevicePx(24.0)));
//! let first = clips.only(ClipLink::rect(rect));
//! assert_eq!(clips.only(ClipLink::rect(rect)), first, "equal chains share an id");
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod clip;
pub mod content;
pub mod corner;
pub mod id;
pub mod spatial;
pub mod table;

pub use crate::clip::{
    ClipLink, ClipNode, ClipTable, KeptClip, MaskSource, ResolvedClip, RoundedTest,
};
pub use crate::content::{Content, ContentHash};
pub use crate::corner::CornerShape;
pub use crate::id::{ClipId, PaintId, PaintSlot, ScrollFrameId, StackingContextId};
pub use crate::spatial::{
    Anchoring, OwnSpace, Placements, PropertyId, PropertyNode, PropertyOwner, PropertyTree,
    SPATIAL_DOMAIN, SpatialId, SpatialNode, SpatialTree,
};
pub use crate::table::{ChangeCoverage, Table, TableVersion};
