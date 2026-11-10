//! Conversion of scene paint values to browser paint values.

use std::collections::BTreeSet;

use wasm_bindgen::JsValue;
use web_sys::CanvasRenderingContext2d;
use zgui_color::{Color, ColorSpace};
use zgui_scene::{Filter, GradientKind, Paint, PaintRef, Scene};

pub(crate) fn warn(warnings: &mut BTreeSet<&'static str>, feature: &'static str) {
    if warnings.insert(feature) {
        web_sys::console::warn_1(&JsValue::from_str(&format!("zgui Canvas 2D: {feature}")));
    }
}

pub(crate) fn color(color: Color) -> String {
    let color = color.to_space(ColorSpace::Srgb);
    let [r, g, b] = color
        .components()
        .map(|value| value.clamp(0.0, 1.0) * 255.0);
    format!("rgba({r},{g},{b},{})", color.alpha().clamp(0.0, 1.0))
}

pub(crate) fn premultiplied(value: [f32; 4]) -> String {
    let [r, g, b, a] = value;
    if a <= 0.0 {
        return "rgba(0,0,0,0)".into();
    }
    color(Color::srgb(r / a, g / a, b / a, a))
}

pub(crate) fn set_fill(
    context: &CanvasRenderingContext2d,
    scene: &Scene,
    reference: PaintRef,
    origin: [f32; 2],
    brush: kurbo::Affine,
    warnings: &mut BTreeSet<&'static str>,
) -> Result<bool, JsValue> {
    let Some(paint) = reference.id().and_then(|id| scene.paints.get(id)) else {
        return Ok(false);
    };
    match paint {
        Paint::Solid(value) => context.set_fill_style_str(&color(*value)),
        Paint::Sides(colors) => context.set_fill_style_str(&color(colors[0])),
        Paint::Gradient {
            kind,
            stops,
            space,
            repeating,
            ..
        } => {
            if *repeating || matches!(kind, GradientKind::Conic { .. }) {
                warn(
                    warnings,
                    "repeating and conic gradients use their mean color",
                );
                if let Some(value) = paint.flat_color() {
                    context.set_fill_style_str(&color(value));
                    return Ok(true);
                }
                return Ok(false);
            }
            if *space != ColorSpace::Srgb {
                warn(warnings, "gradient interpolation uses browser sRGB");
            }
            context.save();
            context.translate(origin[0] as f64, origin[1] as f64)?;
            crate::path::affine(context, brush)?;
            let gradient = match *kind {
                GradientKind::Linear { start, end } => context.create_linear_gradient(
                    start.x.0 as f64,
                    start.y.0 as f64,
                    end.x.0 as f64,
                    end.y.0 as f64,
                ),
                GradientKind::Radial {
                    center,
                    radius_x,
                    radius_y,
                } => {
                    context.translate(center.x.0 as f64, center.y.0 as f64)?;
                    context.scale(radius_x.max(0.0001) as f64, radius_y.max(0.0001) as f64)?;
                    context.create_radial_gradient(0.0, 0.0, 0.0, 0.0, 0.0, 1.0)?
                }
                GradientKind::Conic { .. } => unreachable!(),
            };
            context.restore();
            for stop in stops {
                gradient.add_color_stop(stop.offset.clamp(0.0, 1.0), &color(stop.color))?;
            }
            context.set_fill_style_canvas_gradient(&gradient);
        }
        Paint::Image { .. } => {
            warn(
                warnings,
                "image paint fills are unsupported; image sprites are supported",
            );
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn filters(filters: &[Filter], warnings: &mut BTreeSet<&'static str>) -> String {
    let mut parts = Vec::new();
    for filter in filters {
        parts.push(match *filter {
            Filter::Blur(value) => format!("blur({}px)", value.max(0.0)),
            Filter::DropShadow {
                offset_x,
                offset_y,
                blur,
                color,
            } => format!(
                "drop-shadow({offset_x}px {offset_y}px {}px {})",
                blur.max(0.0),
                premultiplied(color),
            ),
            Filter::Brightness(value) => format!("brightness({value})"),
            Filter::Contrast(value) => format!("contrast({value})"),
            Filter::Grayscale(value) => format!("grayscale({value})"),
            Filter::HueRotate(value) => format!("hue-rotate({value}rad)"),
            Filter::Invert(value) => format!("invert({value})"),
            Filter::Opacity(value) => format!("opacity({value})"),
            Filter::Saturate(value) => format!("saturate({value})"),
            Filter::Sepia(value) => format!("sepia({value})"),
            Filter::Custom { .. } => {
                warn(warnings, "custom shader filters are unsupported");
                continue;
            }
        });
    }
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join(" ")
    }
}

pub(crate) fn blend(blend: peniko::BlendMode) -> &'static str {
    use peniko::{Compose, Mix};
    if blend.compose != Compose::SrcOver {
        return match blend.compose {
            Compose::Copy => "copy",
            Compose::DestOver => "destination-over",
            Compose::SrcIn => "source-in",
            Compose::DestIn => "destination-in",
            Compose::SrcOut => "source-out",
            Compose::DestOut => "destination-out",
            Compose::SrcAtop => "source-atop",
            Compose::DestAtop => "destination-atop",
            Compose::Xor => "xor",
            Compose::Plus | Compose::PlusLighter => "lighter",
            _ => "source-over",
        };
    }
    match blend.mix {
        Mix::Normal => "source-over",
        Mix::Multiply => "multiply",
        Mix::Screen => "screen",
        Mix::Overlay => "overlay",
        Mix::Darken => "darken",
        Mix::Lighten => "lighten",
        Mix::ColorDodge => "color-dodge",
        Mix::ColorBurn => "color-burn",
        Mix::HardLight => "hard-light",
        Mix::SoftLight => "soft-light",
        Mix::Difference => "difference",
        Mix::Exclusion => "exclusion",
        Mix::Hue => "hue",
        Mix::Saturation => "saturation",
        Mix::Color => "color",
        Mix::Luminosity => "luminosity",
    }
}
