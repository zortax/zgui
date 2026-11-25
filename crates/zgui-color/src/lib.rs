//! CSS colour: colour spaces, interpolation between them, and the conversion to the values a
//! renderer draws with.
//!
//! The types are the compositor's own, from `iceblink`, so a colour crosses into a
//! compositor item without conversion. [`Color::to_premultiplied_srgb`] produces the premultiplied,
//! gamma-encoded sRGB every renderer draws with.
//!
//! ```
//! use zgui_color::{Color, ColorSpace, Interpolation, interpolate};
//!
//! let blue = Color::srgb(0.0, 0.0, 1.0, 1.0);
//! let yellow = Color::srgb(1.0, 1.0, 0.0, 1.0);
//! let middle = interpolate(blue, yellow, 0.5, Interpolation::new(ColorSpace::Oklab));
//!
//! let [red, green, blue, alpha] = middle.to_premultiplied_srgb();
//! assert!(alpha == 1.0 && red > 0.0 && green > 0.0 && blue > 0.0);
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub use iceblink::color::{
    Color, ColorSpace, DEFAULT_TOLERANCE, GradientStop, HueInterpolation, Interpolation,
    PremultipliedLinear, WhitePoint, color_mix, color_mix_evenly, densify, densify_with_tolerance,
    gamma, gamma_correction_ratios, gradient, interpolate, mix, space, value as color,
};
