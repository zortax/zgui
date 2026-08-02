//! The RGB spaces: primaries, a transfer function, and the XYZ they are defined against.
//!
//! Six of the fourteen colour spaces are RGB spaces, and they differ in exactly two ways: which
//! matrix takes their linear values to CIE XYZ, and which curve relates their encoded values to
//! those linear ones. Both questions are answered here, so adding a seventh gamut is two constants
//! and one match arm rather than a new conversion path.

pub(crate) mod matrices;
pub(crate) mod transfer;

use crate::convert::matrix::{Matrix3, apply};
use crate::space::ColorSpace;

/// Whether `space` is one of the RGB spaces this module handles.
pub(crate) const fn is_rgb(space: ColorSpace) -> bool {
    matches!(
        space,
        ColorSpace::Srgb
            | ColorSpace::SrgbLinear
            | ColorSpace::DisplayP3
            | ColorSpace::A98Rgb
            | ColorSpace::ProPhotoRgb
            | ColorSpace::Rec2020
    )
}

/// Converts one encoded channel of `space` to linear light.
pub(crate) fn to_linear(space: ColorSpace, value: f32) -> f32 {
    match space {
        ColorSpace::Srgb | ColorSpace::DisplayP3 => transfer::srgb_to_linear(value),
        ColorSpace::A98Rgb => transfer::a98_to_linear(value),
        ColorSpace::ProPhotoRgb => transfer::prophoto_to_linear(value),
        ColorSpace::Rec2020 => transfer::rec2020_to_linear(value),
        _ => value,
    }
}

/// Encodes one linear-light channel in `space`.
pub(crate) fn from_linear(space: ColorSpace, value: f32) -> f32 {
    match space {
        ColorSpace::Srgb | ColorSpace::DisplayP3 => transfer::linear_to_srgb(value),
        ColorSpace::A98Rgb => transfer::linear_to_a98(value),
        ColorSpace::ProPhotoRgb => transfer::linear_to_prophoto(value),
        ColorSpace::Rec2020 => transfer::linear_to_rec2020(value),
        _ => value,
    }
}

/// The matrix taking `space`'s linear values to XYZ under its own white point.
fn to_xyz_matrix(space: ColorSpace) -> &'static Matrix3 {
    match space {
        ColorSpace::DisplayP3 => &matrices::DISPLAY_P3_TO_XYZ,
        ColorSpace::A98Rgb => &matrices::A98_TO_XYZ,
        ColorSpace::ProPhotoRgb => &matrices::PROPHOTO_TO_XYZ,
        ColorSpace::Rec2020 => &matrices::REC2020_TO_XYZ,
        _ => &matrices::SRGB_TO_XYZ,
    }
}

/// The matrix taking XYZ under `space`'s own white point to its linear values.
fn from_xyz_matrix(space: ColorSpace) -> &'static Matrix3 {
    match space {
        ColorSpace::DisplayP3 => &matrices::XYZ_TO_DISPLAY_P3,
        ColorSpace::A98Rgb => &matrices::XYZ_TO_A98,
        ColorSpace::ProPhotoRgb => &matrices::XYZ_TO_PROPHOTO,
        ColorSpace::Rec2020 => &matrices::XYZ_TO_REC2020,
        _ => &matrices::XYZ_TO_SRGB,
    }
}

/// Converts an RGB colour to XYZ referenced to that space's own white point.
pub(crate) fn to_xyz(space: ColorSpace, components: [f32; 3]) -> [f32; 3] {
    let linear = components.map(|value| to_linear(space, value));
    apply(to_xyz_matrix(space), linear)
}

/// Converts XYZ referenced to `space`'s own white point into that RGB space.
pub(crate) fn from_xyz(space: ColorSpace, xyz: [f32; 3]) -> [f32; 3] {
    apply(from_xyz_matrix(space), xyz).map(|value| from_linear(space, value))
}

#[cfg(test)]
mod tests {
    use super::{from_xyz, is_rgb, to_xyz};
    use crate::space::ColorSpace;

    #[test]
    fn the_rgb_spaces_are_exactly_the_six() {
        let count = ColorSpace::ALL.into_iter().filter(|it| is_rgb(*it)).count();
        assert_eq!(count, 6);
    }

    #[test]
    fn every_rgb_space_round_trips_through_xyz() {
        for space in ColorSpace::ALL.into_iter().filter(|it| is_rgb(*it)) {
            let components = [0.2, 0.6, 0.9];
            let back = from_xyz(space, to_xyz(space, components));
            for channel in 0..3 {
                assert!(
                    (back[channel] - components[channel]).abs() < 1e-5,
                    "{space:?} channel {channel} came back as {}",
                    back[channel],
                );
            }
        }
    }

    #[test]
    fn white_is_the_white_point_in_every_rgb_space() {
        for space in ColorSpace::ALL.into_iter().filter(|it| is_rgb(*it)) {
            let xyz = to_xyz(space, [1.0, 1.0, 1.0]);
            let expected = space.white_point().tristimulus();
            for channel in 0..3 {
                assert!(
                    (xyz[channel] - expected[channel]).abs() < 1e-4,
                    "{space:?} channel {channel} is {}",
                    xyz[channel],
                );
            }
        }
    }
}
