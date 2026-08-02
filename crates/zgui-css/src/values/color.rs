//! The one conversion from a cascaded colour into the colour type the rest of the tree draws with.

use zgui_color::{Color, ColorSpace, HueInterpolation, Interpolation};

/// A colour as the cascade leaves it: resolved to one space, with no `currentColor` left in it.
pub use style::color::AbsoluteColor;
/// One of the fourteen spaces a cascaded value can name, as the engine spells it.
pub use style::color::ColorSpace as CascadedColorSpace;
/// The space and hue direction a gradient or a `color-mix()` says it interpolates in.
pub use style::color::mix::ColorInterpolationMethod as ColorInterpolationValue;
/// The computed value of every colour-valued property other than `color` itself.
///
/// Unlike [`AbsoluteColor`] this may still be `currentColor`, or a mix of it with an absolute
/// colour, because those resolve against the element's own `color` and that is not known until the
/// value is used.
pub use style::values::computed::Color as ColorValue;
/// The computed value of `color-scheme`.
pub use style::values::computed::ColorScheme as ColorSchemeValue;
/// The computed value of `color` itself.
pub use style::values::computed::color::ColorPropertyValue;

/// Converts a cascaded colour into this framework's own.
///
/// The cascade resolves a colour into one of the CSS Color 4 spaces and keeps it there — a colour
/// written in `oklch()` stays in Oklch, because converting it early would band a gradient drawn
/// through it. This carries the space across rather than flattening to sRGB, so the decision about
/// when to leave a wide space is made once, where a colour becomes numbers for a renderer.
///
/// ```
/// use zgui_css::values::color::to_color;
/// use zgui_color::ColorSpace;
///
/// let red = zgui_css::values::color::AbsoluteColor::srgb_legacy(255, 0, 0, 1.0);
/// let converted = to_color(&red);
/// assert_eq!(converted.space(), ColorSpace::Srgb);
/// assert_eq!(converted.to_premultiplied_srgb(), [1.0, 0.0, 0.0, 1.0]);
/// ```
///
/// A colour-valued property other than `color` itself may still be `currentColor`, which resolves
/// against the element's own `color`; [`resolve`] is that step, and this is what it calls once the
/// keyword is gone.
pub fn to_color(color: &AbsoluteColor) -> Color {
    let color = match to_space(color.color_space) {
        Some(_) => *color,
        // One space the engine can hold has no counterpart here, because it is not one CSS Color 4
        // defines: linear-light Display P3. It is exact in gamma-encoded Display P3, so the colour
        // is converted rather than reinterpreted, and nothing downstream sees a space it cannot
        // name.
        None => color.to_color_space(style::color::ColorSpace::DisplayP3),
    };
    let components = color.raw_components();
    Color::new(
        to_space(color.color_space).unwrap_or(ColorSpace::DisplayP3),
        [components[0], components[1], components[2]],
        components[3],
    )
}

/// Resolves a colour-valued property against the element's own `color`.
///
/// Every colour-valued property other than `color` may be `currentColor`, or a mix with it, and
/// none of them mean anything until that keyword is replaced. Doing it here rather than at each
/// reader is what keeps a border colour, a shadow colour and a decoration colour agreeing about
/// what `currentColor` was.
///
/// ```
/// use zgui_css::StyleDraft;
/// use zgui_css::values::color::{current, resolve};
///
/// let style = StyleDraft::initial().build();
/// // The initial `border-top-color` is `currentColor`, and the initial `color` is black.
/// let border = resolve(&style.get_border().border_top_color, current(&style));
/// assert_eq!(border.to_premultiplied_srgb(), [0.0, 0.0, 0.0, 1.0]);
/// ```
pub fn resolve(value: &ColorValue, current: &AbsoluteColor) -> Color {
    to_color(&value.resolve_to_absolute(current))
}

/// The element's own computed `color`, which every `currentColor` resolves against.
pub fn current(style: &crate::ComputedStyle) -> &AbsoluteColor {
    &style.get_inherited_text().color
}

/// The interpolation a cascaded `in <space> <hue>` clause asks for.
///
/// Falls back to Oklab with the shorter hue arc when the engine names a space this framework does
/// not have, which is CSS's own default and the only space with no counterpart here is one CSS
/// Color 4 does not define.
///
/// ```
/// use zgui_color::{ColorSpace, HueInterpolation};
/// use zgui_css::values::color::{ColorInterpolationValue, to_interpolation};
///
/// let srgb = to_interpolation(&ColorInterpolationValue::srgb());
/// assert_eq!(srgb.space, ColorSpace::Srgb);
/// assert_eq!(srgb.hue, HueInterpolation::Shorter);
/// ```
pub fn to_interpolation(method: &ColorInterpolationValue) -> Interpolation {
    use style::color::mix::HueInterpolationMethod as Cascaded;
    let space = to_space(method.space).unwrap_or(ColorSpace::Oklab);
    let hue = match method.hue {
        Cascaded::Shorter | Cascaded::Specified => HueInterpolation::Shorter,
        Cascaded::Longer => HueInterpolation::Longer,
        Cascaded::Increasing => HueInterpolation::Increasing,
        Cascaded::Decreasing => HueInterpolation::Decreasing,
    };
    Interpolation { space, hue }
}

/// The colour space one cascaded value names, when this framework has that space.
///
/// Asked twice, about two different things: which space a colour is expressed in, and which space a
/// gradient or a `color-mix()` interpolates in. Both are the same enumeration and both must answer
/// the same, so there is one function rather than one per caller.
///
/// `None` is returned only for linear-light Display P3, which CSS Color 4 does not define and which
/// therefore has no counterpart here; [`to_color`] converts such a colour into gamma-encoded
/// Display P3 rather than reinterpreting it.
///
/// ```
/// use zgui_css::values::color::{CascadedColorSpace, to_space};
/// use zgui_color::ColorSpace;
///
/// assert_eq!(to_space(CascadedColorSpace::Oklab), Some(ColorSpace::Oklab));
/// ```
pub fn to_space(space: CascadedColorSpace) -> Option<ColorSpace> {
    use style::color::ColorSpace as Cascaded;
    Some(match space {
        Cascaded::Srgb => ColorSpace::Srgb,
        Cascaded::Hsl => ColorSpace::Hsl,
        Cascaded::Hwb => ColorSpace::Hwb,
        Cascaded::Lab => ColorSpace::Lab,
        Cascaded::Lch => ColorSpace::Lch,
        Cascaded::Oklab => ColorSpace::Oklab,
        Cascaded::Oklch => ColorSpace::Oklch,
        Cascaded::SrgbLinear => ColorSpace::SrgbLinear,
        Cascaded::DisplayP3 => ColorSpace::DisplayP3,
        Cascaded::A98Rgb => ColorSpace::A98Rgb,
        Cascaded::ProphotoRgb => ColorSpace::ProPhotoRgb,
        Cascaded::Rec2020 => ColorSpace::Rec2020,
        Cascaded::XyzD50 => ColorSpace::XyzD50,
        Cascaded::XyzD65 => ColorSpace::XyzD65,
        Cascaded::DisplayP3Linear => return None,
    })
}
