//! Gradient stops, and how a non-sRGB ramp becomes one a renderer can draw.
//!
//! A CSS gradient names the space its colours are interpolated in — `in oklab`, `in hsl longer
//! hue`, `in srgb-linear` — and the curve that produces in those spaces is not a straight line in
//! sRGB. A rasteriser that blends between stop colours in sRGB therefore cannot draw such a
//! gradient from its author-written stops, however many of them there are: two stops describe a
//! curve, and two stops interpolated in sRGB describe a chord.
//!
//! [`densify()`] closes that gap by putting extra stops along the curve, close enough together that
//! the straight lines between them are indistinguishable from the curve at eight-bit precision.
//! The output is always [`ColorSpace::Srgb`](crate::ColorSpace::Srgb), and the consumer's job is
//! to interpolate it in premultiplied sRGB — the encoding
//! [`Color::to_premultiplied_srgb`](crate::Color::to_premultiplied_srgb) produces and every
//! renderer path here uses.

pub mod densify;

pub use crate::gradient::densify::{DEFAULT_TOLERANCE, densify, densify_with_tolerance};

use crate::color::Color;

/// A colour at a position along a gradient.
///
/// The offset is a fraction of the gradient line, so `0.0` is its start and `1.0` its end. Values
/// outside that range are meaningful — a repeating gradient's stops run past both ends — and are
/// carried through untouched.
///
/// ```
/// use zgui_color::{Color, GradientStop};
///
/// let start = GradientStop::new(0.0, Color::BLACK);
/// let end = GradientStop::new(1.0, Color::WHITE);
/// assert!(start.offset < end.offset);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Where along the gradient line this stop sits, as a fraction.
    pub offset: f32,
    /// The colour at that position.
    pub color: Color,
}

impl GradientStop {
    /// A stop at `offset` with `color`.
    pub const fn new(offset: f32, color: Color) -> Self {
        Self { offset, color }
    }
}
