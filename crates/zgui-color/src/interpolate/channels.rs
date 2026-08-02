//! Premultiplication and its inverse, applied to a colour's channels in whatever space it is in.
//!
//! Interpolation happens on premultiplied values in every space, not only in the RGB ones — a
//! ramp from an opaque lightness to a transparent one has the same problem a ramp to transparent
//! red does. A hue is the exception: it is an angle, and scaling an angle by alpha is meaningless.

use crate::space::ColorSpace;

/// Multiplies every non-hue channel by `alpha`.
pub(crate) fn premultiply(space: ColorSpace, components: [f32; 3], alpha: f32) -> [f32; 3] {
    let hue = space.hue_index();
    let mut out = components;
    for (index, value) in out.iter_mut().enumerate() {
        if Some(index) != hue {
            *value *= alpha;
        }
    }
    out
}

/// Divides every non-hue channel by `alpha`, leaving the channels alone when there is no alpha to
/// divide by.
pub(crate) fn unpremultiply(space: ColorSpace, components: [f32; 3], alpha: f32) -> [f32; 3] {
    if alpha == 0.0 {
        return components;
    }
    let hue = space.hue_index();
    let mut out = components;
    for (index, value) in out.iter_mut().enumerate() {
        if Some(index) != hue {
            *value /= alpha;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{premultiply, unpremultiply};
    use crate::space::ColorSpace;

    #[test]
    fn hue_is_never_scaled() {
        let premultiplied = premultiply(ColorSpace::Lch, [50.0, 30.0, 200.0], 0.5);
        assert_eq!(premultiplied, [25.0, 15.0, 200.0]);
        let premultiplied = premultiply(ColorSpace::Hsl, [200.0, 0.5, 0.5], 0.5);
        assert_eq!(premultiplied, [200.0, 0.25, 0.25]);
    }

    #[test]
    fn the_two_directions_are_inverses_in_every_space() {
        let components = [0.4, 0.5, 0.6];
        for space in ColorSpace::ALL {
            let back = unpremultiply(space, premultiply(space, components, 0.4), 0.4);
            for channel in 0..3 {
                assert!(
                    (back[channel] - components[channel]).abs() < 1e-6,
                    "{space:?}"
                );
            }
        }
    }

    #[test]
    fn a_transparent_colour_keeps_its_channels() {
        assert_eq!(
            unpremultiply(ColorSpace::Srgb, [0.1, 0.2, 0.3], 0.0),
            [0.1, 0.2, 0.3],
        );
    }
}
