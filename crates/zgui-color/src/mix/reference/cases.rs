//! The reference mixes themselves.
//!
//! Each row states two colours, their weights, the space to mix in, and the answer the
//! specification's steps give in that space. They are grouped loosely: the sRGB rows exercise the
//! weighting and alpha rules where the arithmetic is easiest to check by eye, and the rest exercise
//! one space each.

use crate::color::Color;
use crate::interpolate::{HueInterpolation, Interpolation};
use crate::mix::reference::Case;
use crate::space::ColorSpace;

/// A colour in a space, spelled out for the table below.
const fn color(space: ColorSpace, components: [f32; 3], alpha: f32) -> Color {
    Color::new(space, components, alpha)
}

/// An sRGB colour, spelled out for the table below.
const fn srgb(red: f32, green: f32, blue: f32, alpha: f32) -> Color {
    Color::srgb(red, green, blue, alpha)
}

/// Mixing in `space`, taking the shorter hue arc.
const fn in_space(space: ColorSpace) -> Interpolation {
    Interpolation::new(space)
}

/// Mixing in `space`, taking the named hue arc.
const fn round(space: ColorSpace, hue: HueInterpolation) -> Interpolation {
    Interpolation::new(space).with_hue(hue)
}

/// Opaque sRGB red.
const RED: Color = srgb(1.0, 0.0, 0.0, 1.0);
/// Opaque sRGB green.
const GREEN: Color = srgb(0.0, 1.0, 0.0, 1.0);
/// Opaque sRGB blue.
const BLUE: Color = srgb(0.0, 0.0, 1.0, 1.0);

/// The reference mixes.
pub(super) const CASES: &[Case] = &[
    Case {
        name: "an even mix in sRGB is the midpoint of every channel",
        interpolation: in_space(ColorSpace::Srgb),
        first: (RED, 0.5),
        second: (BLUE, 0.5),
        expected: srgb(0.5, 0.0, 0.5, 1.0),
    },
    Case {
        name: "weights place the result along the ramp",
        interpolation: in_space(ColorSpace::Srgb),
        first: (RED, 0.25),
        second: (BLUE, 0.75),
        expected: srgb(0.25, 0.0, 0.75, 1.0),
    },
    Case {
        name: "weights that add up to more than one are still a ratio",
        interpolation: in_space(ColorSpace::Srgb),
        first: (RED, 1.0),
        second: (BLUE, 3.0),
        expected: srgb(0.25, 0.0, 0.75, 1.0),
    },
    Case {
        name: "weights that fall short of one take the shortfall out of alpha",
        interpolation: in_space(ColorSpace::Srgb),
        first: (RED, 0.2),
        second: (BLUE, 0.2),
        expected: srgb(0.5, 0.0, 0.5, 0.4),
    },
    Case {
        name: "mixing with transparent halves the alpha and keeps the colour",
        interpolation: in_space(ColorSpace::Srgb),
        first: (Color::TRANSPARENT, 0.5),
        second: (BLUE, 0.5),
        expected: srgb(0.0, 0.0, 1.0, 0.5),
    },
    Case {
        name: "white and black meet in the middle of the encoded ramp",
        interpolation: in_space(ColorSpace::Srgb),
        first: (Color::WHITE, 0.5),
        second: (Color::BLACK, 0.5),
        expected: srgb(0.5, 0.5, 0.5, 1.0),
    },
    Case {
        name: "a zero weight leaves the other colour untouched",
        interpolation: in_space(ColorSpace::Srgb),
        first: (RED, 0.0),
        second: (BLUE, 1.0),
        expected: srgb(0.0, 0.0, 1.0, 1.0),
    },
    Case {
        name: "a zero second weight leaves the first colour untouched",
        interpolation: in_space(ColorSpace::Srgb),
        first: (RED, 1.0),
        second: (BLUE, 0.0),
        expected: srgb(1.0, 0.0, 0.0, 1.0),
    },
    Case {
        name: "premultiplication tilts a mix towards the more opaque colour",
        interpolation: in_space(ColorSpace::Srgb),
        first: (srgb(1.0, 0.0, 0.0, 0.5), 0.5),
        second: (BLUE, 0.5),
        expected: srgb(1.0 / 3.0, 0.0, 2.0 / 3.0, 0.75),
    },
    Case {
        name: "alpha scales by the total weight, not by each weight",
        interpolation: in_space(ColorSpace::Srgb),
        first: (Color::WHITE, 0.3),
        second: (Color::BLACK, 0.3),
        expected: srgb(0.5, 0.5, 0.5, 0.6),
    },
    Case {
        name: "a tint is a mix with white",
        interpolation: in_space(ColorSpace::Srgb),
        first: (GREEN, 0.75),
        second: (Color::WHITE, 0.25),
        expected: srgb(0.25, 1.0, 0.25, 1.0),
    },
    Case {
        name: "two partly transparent colours mix in proportion to their alphas",
        interpolation: in_space(ColorSpace::Srgb),
        first: (srgb(0.0, 0.0, 1.0, 0.25), 0.5),
        second: (srgb(1.0, 0.0, 0.0, 0.75), 0.5),
        expected: srgb(0.75, 0.0, 0.25, 0.5),
    },
    Case {
        name: "an alpha-scaled mix of transparent colours keeps both rules apart",
        interpolation: in_space(ColorSpace::Srgb),
        first: (srgb(1.0, 0.0, 0.0, 0.5), 0.6),
        second: (srgb(0.0, 0.0, 1.0, 0.5), 0.2),
        expected: srgb(0.75, 0.0, 0.25, 0.4),
    },
    Case {
        name: "linear-light white and black meet at half the light",
        interpolation: in_space(ColorSpace::SrgbLinear),
        first: (Color::WHITE, 0.5),
        second: (Color::BLACK, 0.5),
        expected: color(ColorSpace::SrgbLinear, [0.5, 0.5, 0.5], 1.0),
    },
    Case {
        name: "linear-light primaries mix channel by channel",
        interpolation: in_space(ColorSpace::SrgbLinear),
        first: (RED, 0.5),
        second: (BLUE, 0.5),
        expected: color(ColorSpace::SrgbLinear, [0.5, 0.0, 0.5], 1.0),
    },
    Case {
        name: "linear light premultiplies like every other space",
        interpolation: in_space(ColorSpace::SrgbLinear),
        first: (Color::TRANSPARENT, 0.5),
        second: (Color::WHITE, 0.5),
        expected: color(ColorSpace::SrgbLinear, [1.0, 1.0, 1.0], 0.5),
    },
    Case {
        name: "XYZ D65 mixes towards half the white point",
        interpolation: in_space(ColorSpace::XyzD65),
        first: (Color::WHITE, 0.5),
        second: (Color::BLACK, 0.5),
        expected: color(ColorSpace::XyzD65, [0.475_228, 0.5, 0.544_528_9], 1.0),
    },
    Case {
        name: "XYZ D50 mixes towards half its own white point",
        interpolation: in_space(ColorSpace::XyzD50),
        first: (Color::WHITE, 0.5),
        second: (Color::BLACK, 0.5),
        expected: color(ColorSpace::XyzD50, [0.482_147_85, 0.5, 0.412_552_3], 1.0),
    },
    Case {
        name: "L*a*b* puts mid grey at lightness fifty",
        interpolation: in_space(ColorSpace::Lab),
        first: (Color::WHITE, 0.5),
        second: (Color::BLACK, 0.5),
        expected: color(ColorSpace::Lab, [50.0, 0.0, 0.0], 1.0),
    },
    Case {
        name: "L*a*b* channels are walked independently",
        interpolation: in_space(ColorSpace::Lab),
        first: (color(ColorSpace::Lab, [50.0, 40.0, -30.0], 1.0), 0.5),
        second: (color(ColorSpace::Lab, [70.0, -20.0, 10.0], 1.0), 0.5),
        expected: color(ColorSpace::Lab, [60.0, 10.0, -10.0], 1.0),
    },
    Case {
        name: "a weighted L*a*b* mix lands three quarters along",
        interpolation: in_space(ColorSpace::Lab),
        first: (color(ColorSpace::Lab, [50.0, 40.0, -30.0], 1.0), 0.25),
        second: (color(ColorSpace::Lab, [70.0, -20.0, 10.0], 1.0), 0.75),
        expected: color(ColorSpace::Lab, [65.0, -5.0, 0.0], 1.0),
    },
    Case {
        name: "L*a*b* premultiplies its lightness as well as its axes",
        interpolation: in_space(ColorSpace::Lab),
        first: (color(ColorSpace::Lab, [50.0, 40.0, -30.0], 0.4), 0.5),
        second: (color(ColorSpace::Lab, [70.0, -20.0, 10.0], 0.8), 0.5),
        expected: color(ColorSpace::Lab, [190.0 / 3.0, 0.0, -10.0 / 3.0], 0.6),
    },
    Case {
        name: "LCH takes the short way round and crosses zero",
        interpolation: in_space(ColorSpace::Lch),
        first: (color(ColorSpace::Lch, [50.0, 30.0, 20.0], 1.0), 0.5),
        second: (color(ColorSpace::Lch, [70.0, 50.0, 340.0], 1.0), 0.5),
        expected: color(ColorSpace::Lch, [60.0, 40.0, 0.0], 1.0),
    },
    Case {
        name: "LCH the long way round goes through the opposite hue",
        interpolation: round(ColorSpace::Lch, HueInterpolation::Longer),
        first: (color(ColorSpace::Lch, [50.0, 30.0, 20.0], 1.0), 0.5),
        second: (color(ColorSpace::Lch, [70.0, 50.0, 340.0], 1.0), 0.5),
        expected: color(ColorSpace::Lch, [60.0, 40.0, 180.0], 1.0),
    },
    Case {
        name: "increasing hue travels forwards however far that is",
        interpolation: round(ColorSpace::Lch, HueInterpolation::Increasing),
        first: (color(ColorSpace::Lch, [50.0, 30.0, 20.0], 1.0), 0.5),
        second: (color(ColorSpace::Lch, [70.0, 50.0, 340.0], 1.0), 0.5),
        expected: color(ColorSpace::Lch, [60.0, 40.0, 180.0], 1.0),
    },
    Case {
        name: "decreasing hue travels backwards however far that is",
        interpolation: round(ColorSpace::Lch, HueInterpolation::Decreasing),
        first: (color(ColorSpace::Lch, [50.0, 30.0, 20.0], 1.0), 0.5),
        second: (color(ColorSpace::Lch, [70.0, 50.0, 340.0], 1.0), 0.5),
        expected: color(ColorSpace::Lch, [60.0, 40.0, 0.0], 1.0),
    },
    Case {
        name: "a short LCH arc the other way round zero",
        interpolation: in_space(ColorSpace::Lch),
        first: (color(ColorSpace::Lch, [40.0, 20.0, 350.0], 1.0), 0.5),
        second: (color(ColorSpace::Lch, [60.0, 40.0, 30.0], 1.0), 0.5),
        expected: color(ColorSpace::Lch, [50.0, 30.0, 10.0], 1.0),
    },
    Case {
        name: "Oklab channels are walked independently",
        interpolation: in_space(ColorSpace::Oklab),
        first: (color(ColorSpace::Oklab, [0.5, 0.1, -0.1], 1.0), 0.5),
        second: (color(ColorSpace::Oklab, [0.7, -0.1, 0.1], 1.0), 0.5),
        expected: color(ColorSpace::Oklab, [0.6, 0.0, 0.0], 1.0),
    },
    Case {
        name: "a weighted Oklab mix lands three quarters along",
        interpolation: in_space(ColorSpace::Oklab),
        first: (color(ColorSpace::Oklab, [0.5, 0.1, -0.1], 1.0), 0.25),
        second: (color(ColorSpace::Oklab, [0.7, -0.1, 0.1], 1.0), 0.75),
        expected: color(ColorSpace::Oklab, [0.65, -0.05, 0.05], 1.0),
    },
    Case {
        name: "Oklab premultiplies before it mixes",
        interpolation: in_space(ColorSpace::Oklab),
        first: (color(ColorSpace::Oklab, [0.5, 0.1, -0.1], 0.4), 0.5),
        second: (color(ColorSpace::Oklab, [0.7, -0.1, 0.1], 0.8), 0.5),
        expected: color(
            ColorSpace::Oklab,
            [0.633_333_3, -0.033_333_33, 0.033_333_33],
            0.6,
        ),
    },
    Case {
        name: "Oklab puts white and black at half lightness",
        interpolation: in_space(ColorSpace::Oklab),
        first: (Color::WHITE, 0.5),
        second: (Color::BLACK, 0.5),
        expected: color(ColorSpace::Oklab, [0.5, 0.0, 0.0], 1.0),
    },
    Case {
        name: "mixing transparent black with white in Oklab gives white at half alpha",
        interpolation: in_space(ColorSpace::Oklab),
        first: (Color::TRANSPARENT, 0.5),
        second: (Color::WHITE, 0.5),
        expected: color(ColorSpace::Oklab, [1.0, 0.0, 0.0], 0.5),
    },
    Case {
        name: "Oklch takes the short way round",
        interpolation: in_space(ColorSpace::Oklch),
        first: (color(ColorSpace::Oklch, [0.5, 0.2, 20.0], 1.0), 0.5),
        second: (color(ColorSpace::Oklch, [0.7, 0.1, 320.0], 1.0), 0.5),
        expected: color(ColorSpace::Oklch, [0.6, 0.15, 350.0], 1.0),
    },
    Case {
        name: "Oklch the long way round",
        interpolation: round(ColorSpace::Oklch, HueInterpolation::Longer),
        first: (color(ColorSpace::Oklch, [0.5, 0.2, 20.0], 1.0), 0.5),
        second: (color(ColorSpace::Oklch, [0.7, 0.1, 320.0], 1.0), 0.5),
        expected: color(ColorSpace::Oklch, [0.6, 0.15, 170.0], 1.0),
    },
    Case {
        name: "two Oklch colours of the same hue mix without touching it",
        interpolation: in_space(ColorSpace::Oklch),
        first: (color(ColorSpace::Oklch, [0.4, 0.1, 10.0], 1.0), 0.5),
        second: (color(ColorSpace::Oklch, [0.8, 0.3, 10.0], 1.0), 0.5),
        expected: color(ColorSpace::Oklch, [0.6, 0.2, 10.0], 1.0),
    },
    Case {
        name: "HSL takes the short way round the hue circle",
        interpolation: in_space(ColorSpace::Hsl),
        first: (color(ColorSpace::Hsl, [20.0, 0.5, 0.5], 1.0), 0.5),
        second: (color(ColorSpace::Hsl, [320.0, 0.5, 0.5], 1.0), 0.5),
        expected: color(ColorSpace::Hsl, [350.0, 0.5, 0.5], 1.0),
    },
    Case {
        name: "HSL the long way round",
        interpolation: round(ColorSpace::Hsl, HueInterpolation::Longer),
        first: (color(ColorSpace::Hsl, [20.0, 0.5, 0.5], 1.0), 0.5),
        second: (color(ColorSpace::Hsl, [320.0, 0.5, 0.5], 1.0), 0.5),
        expected: color(ColorSpace::Hsl, [170.0, 0.5, 0.5], 1.0),
    },
    Case {
        name: "a tint in HSL halves the saturation and raises the lightness",
        interpolation: in_space(ColorSpace::Hsl),
        first: (RED, 0.5),
        second: (Color::WHITE, 0.5),
        expected: color(ColorSpace::Hsl, [0.0, 0.5, 0.75], 1.0),
    },
    Case {
        name: "an HSL mix across zero averages saturation and lightness too",
        interpolation: in_space(ColorSpace::Hsl),
        first: (color(ColorSpace::Hsl, [350.0, 0.4, 0.6], 1.0), 0.5),
        second: (color(ColorSpace::Hsl, [10.0, 0.8, 0.2], 1.0), 0.5),
        expected: color(ColorSpace::Hsl, [0.0, 0.6, 0.4], 1.0),
    },
    Case {
        name: "HWB mixes whiteness and blackness like any other channel",
        interpolation: in_space(ColorSpace::Hwb),
        first: (color(ColorSpace::Hwb, [120.0, 0.2, 0.3], 1.0), 0.5),
        second: (color(ColorSpace::Hwb, [240.0, 0.4, 0.1], 1.0), 0.5),
        expected: color(ColorSpace::Hwb, [180.0, 0.3, 0.2], 1.0),
    },
    Case {
        name: "HWB white and black meet at half of each",
        interpolation: in_space(ColorSpace::Hwb),
        first: (Color::WHITE, 0.5),
        second: (Color::BLACK, 0.5),
        expected: color(ColorSpace::Hwb, [0.0, 0.5, 0.5], 1.0),
    },
    Case {
        name: "Display P3 mixes in its own primaries",
        interpolation: in_space(ColorSpace::DisplayP3),
        first: (color(ColorSpace::DisplayP3, [1.0, 0.0, 0.0], 1.0), 0.5),
        second: (color(ColorSpace::DisplayP3, [0.0, 0.0, 1.0], 1.0), 0.5),
        expected: color(ColorSpace::DisplayP3, [0.5, 0.0, 0.5], 1.0),
    },
    Case {
        name: "Rec. 2020 has the same white and black as sRGB",
        interpolation: in_space(ColorSpace::Rec2020),
        first: (Color::WHITE, 0.5),
        second: (Color::BLACK, 0.5),
        expected: color(ColorSpace::Rec2020, [0.5, 0.5, 0.5], 1.0),
    },
    Case {
        name: "Adobe RGB has the same white and black as sRGB",
        interpolation: in_space(ColorSpace::A98Rgb),
        first: (Color::WHITE, 0.5),
        second: (Color::BLACK, 0.5),
        expected: color(ColorSpace::A98Rgb, [0.5, 0.5, 0.5], 1.0),
    },
    Case {
        name: "ProPhoto's D50 white is still white after adaptation",
        interpolation: in_space(ColorSpace::ProPhotoRgb),
        first: (Color::WHITE, 0.5),
        second: (Color::BLACK, 0.5),
        expected: color(ColorSpace::ProPhotoRgb, [0.5, 0.5, 0.5], 1.0),
    },
];
