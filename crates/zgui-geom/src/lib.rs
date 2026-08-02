//! Coordinate spaces, geometry and the plain-old-data layouts every other zgui crate speaks.
//!
//! Three ideas hold this crate together.
//!
//! **Scalars are units, not bare numbers.** [`CssPx`] is a CSS pixel, [`DevicePx`] is a physical
//! pixel on the output surface, and [`Au`] is an exact 1/60th of a CSS pixel used wherever
//! accumulated rounding error would be visible. They are separate types, so a value measured in
//! one can never silently stand in for another.
//!
//! **Geometry is tagged with the space it was measured in.** [`Point`], [`Size`] and [`Rect`]
//! carry a zero-sized marker — [`Css`], [`Device`] or [`Layout`] — that costs nothing at runtime
//! and makes mixing spaces a compile error:
//!
//! ```compile_fail
//! # use zgui_geom::{Css, CssPx, Device, Point};
//! let css: Point<CssPx, Css> = Point::new(CssPx(1.0), CssPx(2.0));
//! let device: Point<CssPx, Device> = Point::new(CssPx(1.0), CssPx(2.0));
//! let _ = css + device;
//! ```
//!
//! Converting between spaces goes through a [`Scale`], which names both endpoints, so the
//! conversion is checked as well:
//!
//! ```
//! use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Scale};
//!
//! let scale: Scale<Css, Device> = Scale::new(2.0);
//! let css: Point<CssPx, Css> = Point::new(CssPx(3.0), CssPx(4.0));
//! let device: Point<DevicePx, Device> = css * scale;
//! assert_eq!(device, Point::new(DevicePx(6.0), DevicePx(8.0)));
//! ```
//!
//! **Everything that can reach the GPU has a fixed, padding-free layout.** Every type here is
//! `#[repr(C)]` and implements [`bytemuck::Pod`], and the byte offset, size and alignment of each
//! one is asserted at compile time, so a struct can be memory-mapped into a vertex or storage
//! buffer without a copy and without ever reading an uninitialised padding byte.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`mod@unit`] | [`CssPx`], [`DevicePx`], [`Au`], [`Scale`] and the [`Unit`] trait |
//! | [`space`] | The [`Css`], [`Device`] and [`Layout`] markers |
//! | [`point`], [`size`], [`rect`], [`edges`] | Space-tagged geometry and box insets |
//! | [`corners`] | [`Corners`] and the elliptical radius pair [`Vec2`] |
//! | [`transform`] | [`Affine2`], [`Matrix4`] and [`Decomposed`] |
//! | [`snap`] | The device-pixel snapping policy |
//! | [`pod`] | The compile-time layout assertions |

#![deny(missing_docs)]
// Plain-old-data promises for the types that cross to the GPU are the one thing this crate needs
// `unsafe` for; every such promise lives in `pod` and carries its own safety argument.
#![deny(unsafe_code)]

pub mod corners;
pub mod edges;
pub mod pod;
pub mod point;
pub mod rect;
pub mod size;
pub mod snap;
pub mod space;
pub mod transform;
pub mod unit;

pub use crate::corners::{Corners, Vec2};
pub use crate::edges::Edges;
pub use crate::point::Point;
pub use crate::rect::Rect;
pub use crate::size::Size;
pub use crate::snap::{cover_bounds, snap_bounds, snap_edges, snap_stroke};
pub use crate::space::{Css, Device, Layout, Space};
pub use crate::transform::{Affine2, Decomposed, Matrix4, transformed_bounds};
pub use crate::unit::{Au, CssPx, DevicePx, Scale, Unit};
