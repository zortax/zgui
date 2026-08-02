//! Conversions checked against published CSS Color 4 values.
//!
//! Round-tripping proves a conversion is reversible, which a pair of matching mistakes also
//! manages. These cases pin the numbers themselves: sRGB red, the primary every colour-management
//! reference tabulates, plus the white point, the achromatic axis, and a colour that only exists
//! outside the sRGB gamut.

use crate::color::Color;
use crate::convert::tests::assert_close;
use crate::space::ColorSpace;

/// How far a converted channel may sit from its published value.
///
/// The published values are quoted to four or five decimal places and are computed from matrices
/// carried to more digits than a `f32` holds, so this is the agreement to expect, not a slack
/// bound.
const TOLERANCE: f32 = 2e-3;

/// The same tolerance on the L\*a\*b\* scale, whose channels run to a hundred rather than to one.
const LAB_TOLERANCE: f32 = 0.2;

/// Opaque sRGB red, the colour every reference tabulates.
const RED: Color = Color::srgb(1.0, 0.0, 0.0, 1.0);

#[test]
fn srgb_red_in_the_wide_gamut_rgb_spaces() {
    let cases = [
        (ColorSpace::DisplayP3, [0.917_48, 0.200_45, 0.138_60]),
        (ColorSpace::A98Rgb, [0.858_63, 0.0, 0.0]),
        (ColorSpace::ProPhotoRgb, [0.702_24, 0.275_69, 0.103_52]),
        (ColorSpace::Rec2020, [0.791_95, 0.231_00, 0.073_76]),
    ];
    for (space, expected) in cases {
        assert_close(
            RED.to_space(space).components(),
            expected,
            TOLERANCE,
            space.keyword(),
        );
    }
}

#[test]
fn srgb_red_in_the_perceptual_spaces() {
    assert_close(
        RED.to_space(ColorSpace::Oklab).components(),
        [0.627_96, 0.224_86, 0.125_85],
        TOLERANCE,
        "oklab",
    );
    assert_close(
        RED.to_space(ColorSpace::Oklch).components(),
        [0.627_96, 0.257_68, 29.234],
        TOLERANCE.max(0.05),
        "oklch",
    );
    assert_close(
        RED.to_space(ColorSpace::Lab).components(),
        [54.290_5, 80.812_4, 69.891_1],
        LAB_TOLERANCE,
        "lab",
    );
    assert_close(
        RED.to_space(ColorSpace::Lch).components(),
        [54.290_5, 106.837_5, 40.857_6],
        LAB_TOLERANCE,
        "lch",
    );
}

#[test]
fn srgb_red_in_the_reference_spaces() {
    assert_close(
        RED.to_space(ColorSpace::XyzD65).components(),
        [0.412_39, 0.212_64, 0.019_33],
        TOLERANCE,
        "xyz-d65",
    );
    assert_close(
        RED.to_space(ColorSpace::XyzD50).components(),
        [0.436_07, 0.222_49, 0.013_93],
        TOLERANCE,
        "xyz-d50",
    );
    assert_close(
        RED.to_space(ColorSpace::Hsl).components(),
        [0.0, 1.0, 0.5],
        TOLERANCE,
        "hsl",
    );
    assert_close(
        RED.to_space(ColorSpace::Hwb).components(),
        [0.0, 0.0, 0.0],
        TOLERANCE,
        "hwb",
    );
}

#[test]
fn the_green_and_blue_primaries_in_oklab() {
    let green = Color::srgb(0.0, 1.0, 0.0, 1.0);
    assert_close(
        green.to_space(ColorSpace::Oklab).components(),
        [0.866_44, -0.233_89, 0.179_50],
        TOLERANCE,
        "oklab green",
    );
    let blue = Color::srgb(0.0, 0.0, 1.0, 1.0);
    assert_close(
        blue.to_space(ColorSpace::Oklab).components(),
        [0.452_01, -0.032_46, -0.311_53],
        TOLERANCE,
        "oklab blue",
    );
}

#[test]
fn white_is_the_white_point_of_every_space() {
    let cases = [
        (ColorSpace::XyzD65, [0.950_46, 1.0, 1.089_06]),
        (ColorSpace::XyzD50, [0.964_30, 1.0, 0.825_10]),
        (ColorSpace::Lab, [100.0, 0.0, 0.0]),
        (ColorSpace::Oklab, [1.0, 0.0, 0.0]),
        (ColorSpace::DisplayP3, [1.0, 1.0, 1.0]),
        (ColorSpace::A98Rgb, [1.0, 1.0, 1.0]),
        (ColorSpace::ProPhotoRgb, [1.0, 1.0, 1.0]),
        (ColorSpace::Rec2020, [1.0, 1.0, 1.0]),
    ];
    for (space, expected) in cases {
        let tolerance = if space == ColorSpace::Lab {
            LAB_TOLERANCE
        } else {
            TOLERANCE
        };
        assert_close(
            Color::WHITE.to_space(space).components(),
            expected,
            tolerance,
            space.keyword(),
        );
    }
}

#[test]
fn a_display_p3_red_is_outside_the_srgb_gamut_and_says_so() {
    // The whole point of the wide-gamut spaces: this colour has no sRGB representation, and the
    // conversion reports where it would be rather than clipping it to something else.
    let p3_red = Color::new(ColorSpace::DisplayP3, [1.0, 0.0, 0.0], 1.0);
    assert_close(
        p3_red.to_space(ColorSpace::Srgb).components(),
        [1.093_10, -0.226_77, -0.150_12],
        TOLERANCE,
        "display-p3 red in srgb",
    );
}

#[test]
fn mid_grey_has_no_chroma_anywhere() {
    let grey = Color::srgb(0.5, 0.5, 0.5, 1.0);
    assert!(grey.to_space(ColorSpace::Lch).components()[1] < 1e-2);
    assert!(grey.to_space(ColorSpace::Oklch).components()[1] < 1e-4);
    assert_eq!(grey.to_space(ColorSpace::Hsl).components()[1], 0.0);
}
