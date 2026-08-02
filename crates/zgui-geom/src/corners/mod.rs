//! Per-corner values, and the elliptical radii CSS actually specifies.
//!
//! [`Corners<T>`] holds one `T` per corner of a box. The value that matters here is
//! `Corners<Vec2<T>>`: CSS `border-radius` gives every corner a *horizontal* and a *vertical*
//! radius, so `border-radius: 20px / 10px` describes an ellipse quadrant and not a circular arc.
//! A single scalar per corner cannot express that, so the radius type in this crate is a pair and
//! there is no scalar alternative to reach for. [`Vec2::splat`] is how a circular corner is
//! written.

pub mod elliptical;
pub mod radius;

pub use crate::corners::elliptical::Vec2;
pub use crate::corners::radius::Corners;
