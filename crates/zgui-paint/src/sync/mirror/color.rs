//! zgui colours and paints in the compositor's vocabulary.

use std::sync::Arc;

use crate::paints::GradientKind;
use compositor_types::compositor_color as target;
use compositor_types::{Color, Gradient, GradientShape, GradientStop, Interpolation, Paint};

/// Returns `paint` as a compositor paint, or `None` for an image paint.
pub(crate) fn paint(paint: &crate::paints::Paint) -> Option<Paint> {
    match paint {
        crate::paints::Paint::Solid(color) => Some(Paint::Solid(premultiplied(*color))),
        crate::paints::Paint::Sides(colors) => Some(Paint::Sides(colors.map(premultiplied))),
        crate::paints::Paint::Gradient {
            kind,
            stops,
            space,
            hue,
            repeating,
        } => Some(Paint::Gradient(Gradient {
            shape: shape(*kind),
            stops: stops
                .iter()
                .map(|stop| GradientStop {
                    offset: stop.offset,
                    color: color(stop.color),
                })
                .collect::<Arc<[_]>>(),
            interpolation: Interpolation {
                space: color_space(*space),
                hue: hue_interpolation(*hue),
            },
            repeat: *repeating,
        })),
    }
}

/// Returns `color` premultiplied in gamma-encoded sRGB.
pub(crate) fn premultiplied(color: zgui_color::Color) -> Color {
    Color(color.to_premultiplied_srgb())
}

/// Returns `color` in the same space with the same channels.
fn color(color: zgui_color::Color) -> target::Color {
    target::Color::new(
        color_space(color.space()),
        color.components(),
        color.alpha(),
    )
}

/// Returns the geometry of a gradient.
fn shape(kind: GradientKind) -> GradientShape {
    match kind {
        GradientKind::Linear { start, end } => GradientShape::Linear {
            start: [start.x.0, start.y.0],
            end: [end.x.0, end.y.0],
        },
        GradientKind::Radial {
            center,
            radius_x,
            radius_y,
        } => GradientShape::Radial {
            center: [center.x.0, center.y.0],
            radius: [radius_x, radius_y],
        },
        GradientKind::Conic { center, from_angle } => GradientShape::Conic {
            center: [center.x.0, center.y.0],
            angle: from_angle,
        },
    }
}

/// Returns the colour space of the same name.
fn color_space(space: zgui_color::ColorSpace) -> target::ColorSpace {
    use target::ColorSpace as To;
    use zgui_color::ColorSpace as From;
    match space {
        From::Srgb => To::Srgb,
        From::SrgbLinear => To::SrgbLinear,
        From::Hsl => To::Hsl,
        From::Hwb => To::Hwb,
        From::Lab => To::Lab,
        From::Lch => To::Lch,
        From::Oklab => To::Oklab,
        From::Oklch => To::Oklch,
        From::DisplayP3 => To::DisplayP3,
        From::A98Rgb => To::A98Rgb,
        From::ProPhotoRgb => To::ProPhotoRgb,
        From::Rec2020 => To::Rec2020,
        From::XyzD50 => To::XyzD50,
        From::XyzD65 => To::XyzD65,
    }
}

/// Returns the hue direction of the same name.
fn hue_interpolation(hue: zgui_color::HueInterpolation) -> target::HueInterpolation {
    use target::HueInterpolation as To;
    use zgui_color::HueInterpolation as From;
    match hue {
        From::Shorter => To::Shorter,
        From::Longer => To::Longer,
        From::Increasing => To::Increasing,
        From::Decreasing => To::Decreasing,
    }
}
