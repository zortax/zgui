//! The primitives themselves: one plain-old-data instance struct per kind.
//!
//! Every struct here is `#[repr(C)]` and [`bytemuck::Pod`], with explicit `f32`, `[f32; 4]` and
//! `u32` fields and no padding anywhere — which is checked field by field, at compile time, by the
//! table in [`mod@layout`]. Two things follow. A batch of instances copies into a buffer as bytes
//! with no per-instance work and no read of an uninitialised padding byte. And a struct whose Rust
//! layout only *happens* to agree with a shader's declaration cannot drift, because the agreement
//! is asserted rather than observed.
//!
//! Every instance begins with its [`DrawOrder`](crate::DrawOrder) at offset zero and ends with its
//! clip and transform indices. Everything in between is what that kind of primitive needs.
//!
//! # `bounds` is the ink, not the geometry
//!
//! A primitive's `bounds` field is the rectangle draw order and culling are computed from, so it is
//! everything the primitive *paints*. For a quad that is its rectangle; for a shadow it is the
//! rectangle already dilated by the blur, because a shadow paints well outside the box that cast
//! it. Under-reporting it leaves stale pixels behind, which is why it is stated here rather than
//! left to each caller to decide.

pub mod corner;
pub mod decoration;
pub mod external;
pub mod kind;
pub mod layout;
pub mod quad;
pub mod shaded;
pub mod shadow;
pub mod sprite;

pub use crate::prim::corner::CornerShape;
pub use crate::prim::decoration::{Decoration, DecorationStyle};
pub use crate::prim::external::{ExternalQuad, ExternalTextureId};
pub use crate::prim::kind::PrimitiveKind;
pub use crate::prim::quad::{BorderStyle, Quad};
pub use crate::prim::shaded::ShadedQuad;
pub use crate::prim::shadow::Shadow;
pub use crate::prim::sprite::{ColorSprite, MonoSprite, Resource, SpriteTile, SubpixelSprite};
