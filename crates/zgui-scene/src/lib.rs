//! The display list: everything a frame draws, as a value, with no renderer in sight.
//!
//! A [`Scene`] is what the paint stage produces and what a renderer consumes. It is deliberately a
//! *value* rather than a renderer's internal state, and three things follow from that: the paint
//! stage is testable with no GPU, a scene can be printed as a stable transcript and diffed in a
//! review, and a second renderer consumes exactly the same input as the first.
//!
//! # How it is laid out
//!
//! One vector per primitive kind, not one vector of an enum. A batch of quads is then a contiguous
//! slice that copies straight into an instance buffer, and every instance struct is `#[repr(C)]`,
//! [`bytemuck::Pod`], and checked field by field by the layout table in [`prim::layout`] — so a
//! struct can be memory-mapped into a buffer without ever reading an uninitialised padding byte,
//! and a reordered field is a build failure rather than a rendering artefact.
//!
//! Everything variable-sized lives in a side table addressed by index from the instance:
//! [`ClipTable`], [`PaintTable`] and [`TextPaintTable`]; the matrix a primitive is drawn under is
//! addressed the same way, through [`SpatialTree`]. An N-stop `conic-gradient` therefore costs a
//! quad exactly as many instance bytes as a flat colour does.
//!
//! # What the tables promise
//!
//! The interning tables are **persistent maps with stable ids** — not per-frame vectors. An entry
//! is interned by content, an id is stable for as long as anything refers to it, entries are marked
//! as used per frame, and eviction takes the coldest generation. That is forced rather than tidy: a
//! fragment replayed from a previous frame's recorded operations carries *that* frame's indices, so
//! per-frame tables would draw one fragment with another fragment's paint or clip — visual
//! corruption with no error anywhere.
//!
//! [`SpatialTree`] promises the same stability by a different route: a coordinate system is named
//! after the box that establishes it rather than after the matrix it holds, so the name survives
//! the matrix being written and a slot that comes back carries an occupancy counter that keeps a
//! stale name from resolving.
//!
//! # Ordering, and what equal order means
//!
//! Draw order comes from [`BoundsTree`]: inserting a rectangle returns one more than the highest
//! order of anything it intersects. Disjoint content therefore reuses low orders and batches
//! together, and — the property everything else leans on — **two primitives at equal order are
//! provably non-overlapping**. Painting order itself is the caller's: this crate assigns numbers,
//! and correct CSS painting order comes from emitting primitives in the right sequence.
//!
//! # Vector passes
//!
//! Vector content is rasterised elsewhere and composited back in at exactly the right point in the
//! order. *Where* those points are is decided here, in [`ScenePassPlan`], from the display list,
//! the bounds tree and the damage set — never by the rasteriser. A pass count is therefore an
//! assertion about the scene, checkable with no device at all.
//!
//! ```
//! use zgui_bits::DamageSet;
//! use zgui_geom::{Device, DevicePx, Point, Rect, Size};
//! use zgui_scene::{PaintRef, Quad, Scene};
//!
//! let mut scene = Scene::new();
//! scene.begin_frame(Size::new(256, 256));
//!
//! let bounds: Rect<DevicePx, Device> =
//!     Rect::new(Point::new(DevicePx(0.0), DevicePx(0.0)), Size::new(DevicePx(64.0), DevicePx(24.0)));
//! let paint = scene.paints.solid(zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0));
//! let fill = PaintRef::solid(paint);
//!
//! let first = scene.push_quad(Quad::filled(bounds, fill)).unwrap();
//! let overlapping = scene.push_quad(Quad::filled(bounds, fill)).unwrap();
//! assert!(overlapping > first, "overlapping content sorts above");
//!
//! scene.finish(&DamageSet::full());
//! assert_eq!(scene.pass_plan().passes.len(), 0, "no vector items, no vector work");
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod batch;
pub mod clip;
pub mod content;
pub mod group;
pub mod id;
pub mod invariant;
pub mod ops;
pub mod order;
pub mod paint;
pub mod pass;
pub mod place;
pub mod prim;
pub mod resource;
pub mod scene;
pub mod spatial;
pub mod table;
pub mod vector;

/// Path geometry, re-exported because it is part of this crate's vocabulary.
///
/// A vector item's geometry *is* a `kurbo` path: it is pure data with no graphics API in it, so
/// carrying it through the display list unchanged means a rasteriser consumes exactly what the
/// paint stage produced. Re-exporting it is what lets a consumer name that geometry without taking
/// its own dependency on a version that might not be this one.
pub use kurbo;
/// Paint vocabulary — fill rules and blend modes — re-exported for the same reason as [`kurbo`].
pub use peniko;

pub use crate::batch::{Batch, Batches};
pub use crate::clip::{ClipLink, ClipNode, ClipTable, MaskSource, ResolvedClip, RoundedTest};
pub use crate::content::{Content, ContentHash};
pub use crate::group::{BackdropFilter, Filter, GroupBoundary, read_extent};
pub use crate::id::{
    ClipId, DrawOrder, PaintId, PaintSlot, ScrollFrameId, StackingContextId, VectorId,
};
pub use crate::ops::PaintOp;
pub use crate::order::BoundsTree;
pub use crate::paint::{
    GradientKind, Paint, PaintKind, PaintRef, PaintTable, TextPaint, TextPaintTable,
};
pub use crate::pass::{Overlap, PassWarning, PlannedItem, PlannedPass, ScenePassPlan};
pub use crate::place::Placement;
pub use crate::place::band::{Travel, Travels};
pub use crate::prim::{
    ColorSprite, Decoration, ExternalQuad, ExternalTextureId, MonoSprite, PrimitiveKind, Quad,
    Resource, Shadow, SpriteTile, SubpixelSprite,
};
pub use crate::resource::{ResourceGeneration, ResourceKey, ResourceKind, ResourceRegistry};
pub use crate::scene::{Scene, SpatialFault};
pub use crate::spatial::{
    Anchoring, OwnSpace, Placements, PropertyId, PropertyNode, PropertyOwner, PropertyTree,
    SPATIAL_DOMAIN, SpatialId, SpatialNode, SpatialTree,
};
pub use crate::table::{ChangeCoverage, Table, TableVersion};
pub use crate::vector::VectorItem;
pub use crate::vector::clip::VectorClip;
pub use crate::vector::stroke::VectorStroke;
