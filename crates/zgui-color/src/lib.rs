//! CSS colour: fourteen colour spaces, interpolation between them, and one conversion to the
//! values a renderer draws with.
//!
//! # One encoding decision, made once
//!
//! A colour has to become four numbers before anything can be drawn, and the choice of *which*
//! four — linear light or gamma-encoded, straight alpha or premultiplied — has to be the same
//! everywhere or images composite subtly wrongly and nobody can say where. This crate makes that
//! choice in exactly one function, [`Color::to_premultiplied_srgb`], which produces premultiplied,
//! gamma-encoded sRGB. It is the only thing here that turns a colour into an array of floats, so
//! there is nowhere else for a second answer to come from.
//!
//! [`Color::to_premultiplied_linear`] sits beside it and is deliberately *not* a renderer path: it
//! serves interpolation that has to happen in linear light, and its result cannot be read out as
//! numbers at all — see [`PremultipliedLinear`].
//!
//! # What is here
//!
//! | Module | Contents |
//! |---|---|
//! | [`space`] | [`ColorSpace`], the fourteen spaces, and what their channels mean |
//! | [`color`] | [`Color`] itself, conversion between spaces, and the two premultiplications |
//! | [`mod@interpolate`] | [`interpolate()`], [`Interpolation`] and [`HueInterpolation`] |
//! | [`mod@mix`] | [`color_mix`] and [`color_mix_evenly`] |
//! | [`gradient`] | [`GradientStop`] and [`densify()`] |
//! | [`mod@gamma`] | [`gamma_correction_ratios`] for text rendering |
//!
//! ```
//! use zgui_color::{Color, ColorSpace, Interpolation, interpolate};
//!
//! // A gradient from blue to yellow, interpolated the way CSS interpolates by default.
//! let blue = Color::srgb(0.0, 0.0, 1.0, 1.0);
//! let yellow = Color::srgb(1.0, 1.0, 0.0, 1.0);
//! let middle = interpolate(blue, yellow, 0.5, Interpolation::new(ColorSpace::Oklab));
//!
//! // Whatever space a colour was computed in, one function turns it into renderer input.
//! let [red, green, blue, alpha] = middle.to_premultiplied_srgb();
//! assert!(alpha == 1.0 && red > 0.0 && green > 0.0 && blue > 0.0);
//! ```
//!
//! # Gradients outside sRGB
//!
//! A rasteriser interpolates between the stop colours it is given, in sRGB. A CSS gradient may ask
//! for its ramp in Oklab, in HSL the long way round the hue circle, or in linear light, and none of
//! those are straight lines in sRGB. [`densify()`] resolves that by adding stops along the true curve
//! until the straight lines between them are within an eight-bit step of it.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod color;
mod convert;
pub mod gamma;
pub mod gradient;
pub mod interpolate;
pub mod mix;
pub mod space;

pub use crate::color::{Color, PremultipliedLinear};
pub use crate::gamma::gamma_correction_ratios;
pub use crate::gradient::{DEFAULT_TOLERANCE, GradientStop, densify, densify_with_tolerance};
pub use crate::interpolate::{HueInterpolation, Interpolation, interpolate};
pub use crate::mix::{color_mix, color_mix_evenly};
pub use crate::space::{ColorSpace, WhitePoint};
