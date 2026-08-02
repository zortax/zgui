//! What fills and strokes a path, in the path renderer's own vocabulary.

use peniko::{Brush, Color as PenikoColor, ColorStop, Extend, Gradient};
use zgui_color::{Color, ColorSpace, Interpolation, densify};
use zgui_scene::{GradientKind, Paint, PaintRef, PaintTable};

/// A brush, and the transform its ramp is measured in when it needs one.
pub struct Painted {
    /// The brush itself.
    pub brush: Brush,
    /// The transform applied to the brush alone, for a ramp whose shape is not the one the brush
    /// vocabulary can describe directly.
    pub transform: Option<kurbo::Affine>,
}

/// The brush `reference` names, or `None` when it paints nothing this can express.
///
/// A sampled image is the case it cannot: what a display list keeps of a decoded image is a tile of
/// a texture the path renderer has no access to, so an image-painted path is reported as unpainted
/// rather than drawn in some other colour.
pub fn of(reference: PaintRef, paints: &PaintTable) -> Option<Painted> {
    let entry = paints.get(reference.id()?)?;
    match entry {
        Paint::Solid(color) => Some(Painted {
            brush: Brush::Solid(solid(*color)),
            transform: None,
        }),
        Paint::Gradient {
            kind,
            stops,
            space,
            hue,
            repeating,
        } => {
            // The ramp is walked in the space CSS asked for and handed over as sRGB stops, because
            // the path renderer interpolates in sRGB and a ramp asked for in Oklab is not a straight
            // line there. Densifying is what makes the two agree to within an eight-bit step.
            let interpolation = Interpolation::new(*space).with_hue(*hue);
            let dense = densify(stops, interpolation);
            let ramp: Vec<ColorStop> = dense
                .iter()
                .map(|stop| ColorStop {
                    offset: stop.offset,
                    color: peniko::color::DynamicColor::from_alpha_color(solid(stop.color)),
                })
                .collect();
            let extend = if *repeating {
                Extend::Repeat
            } else {
                Extend::Pad
            };
            let (gradient, transform) = shape(*kind);
            Some(Painted {
                brush: Brush::Gradient(gradient.with_stops(ramp.as_slice()).with_extend(extend)),
                transform,
            })
        }
        Paint::Image { .. } => None,
    }
}

/// One colour, as gamma-encoded sRGB bytes with straight alpha.
///
/// Straight and encoded, both deliberately: the path renderer blends the non-linear values it is
/// handed and writes straight alpha, so the numbers CSS names are the numbers it gets and the
/// composite is the one place the premultiplication happens.
pub fn solid(color: Color) -> PenikoColor {
    let srgb = color.to_space(ColorSpace::Srgb);
    let [red, green, blue] = srgb.components();
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    PenikoColor::from_rgba8(
        byte(red),
        byte(green),
        byte(blue),
        byte(srgb.alpha().clamp(0.0, 1.0)),
    )
}

/// The gradient shape, and the transform an elliptical one needs.
///
/// A radial ramp in the brush vocabulary has one scalar radius, and CSS radial gradients are
/// elliptical, so the second radius becomes a scale about the centre applied to the brush alone.
fn shape(kind: GradientKind) -> (Gradient, Option<kurbo::Affine>) {
    match kind {
        GradientKind::Linear { start, end } => (
            Gradient::new_linear(
                (f64::from(start.x.0), f64::from(start.y.0)),
                (f64::from(end.x.0), f64::from(end.y.0)),
            ),
            None,
        ),
        GradientKind::Radial {
            center,
            radius_x,
            radius_y,
        } => {
            let origin = kurbo::Point::new(f64::from(center.x.0), f64::from(center.y.0));
            let gradient = Gradient::new_radial(origin, radius_x.max(f32::MIN_POSITIVE));
            let scale = f64::from(radius_y / radius_x.max(f32::MIN_POSITIVE));
            let transform = (scale != 1.0 && scale.is_finite()).then(|| {
                kurbo::Affine::translate(origin.to_vec2())
                    * kurbo::Affine::scale_non_uniform(1.0, scale)
                    * kurbo::Affine::translate(-origin.to_vec2())
            });
            (gradient, transform)
        }
        GradientKind::Conic { center, from_angle } => (
            Gradient::new_sweep(
                (f64::from(center.x.0), f64::from(center.y.0)),
                from_angle,
                from_angle + std::f32::consts::TAU,
            ),
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use zgui_color::{Color, ColorSpace};
    use zgui_scene::{Paint, PaintTable};

    use super::{of, solid};

    #[test]
    fn a_colour_reaches_the_rasteriser_as_the_bytes_css_names_it_by() {
        // Not premultiplied and not linearised: half-transparent mid grey is 128 with alpha 128,
        // because the composite premultiplies and every blend here is in gamma space.
        let color = solid(Color::srgb_u8(128, 128, 128, 128));
        assert_eq!(color.to_rgba8().to_u8_array(), [128, 128, 128, 128]);
    }

    #[test]
    fn a_ramp_asked_for_in_oklab_arrives_as_stops_along_the_oklab_curve() {
        let mut paints = PaintTable::new();
        let stops = [
            zgui_color::GradientStop::new(0.0, Color::srgb_u8(0, 0, 255, 255)),
            zgui_color::GradientStop::new(1.0, Color::srgb_u8(255, 255, 0, 255)),
        ];
        let reference = paints.add(Paint::Gradient {
            kind: zgui_scene::GradientKind::Linear {
                start: zgui_geom::Point::new(zgui_geom::DevicePx(0.0), zgui_geom::DevicePx(0.0)),
                end: zgui_geom::Point::new(zgui_geom::DevicePx(64.0), zgui_geom::DevicePx(0.0)),
            },
            stops: stops.into_iter().collect(),
            space: ColorSpace::Oklab,
            hue: zgui_color::HueInterpolation::Shorter,
            repeating: false,
        });
        let painted = of(reference, &paints).expect("a gradient is expressible");
        let peniko::Brush::Gradient(gradient) = painted.brush else {
            panic!("a gradient paint is a gradient brush");
        };
        assert!(
            gradient.stops.len() > 2,
            "a ramp that is a curve in sRGB has to arrive as more than its two endpoints, or the \
             rasteriser interpolates a straight line between them instead"
        );
    }

    #[test]
    fn an_image_paint_reports_that_it_cannot_be_drawn_rather_than_being_drawn_wrongly() {
        let mut paints = PaintTable::new();
        let tile = zgui_atlas::AtlasTile {
            texture: zgui_atlas::TextureId::new(zgui_atlas::TextureKind::Color, 0),
            tile: zgui_atlas::TileId(0),
            bounds: zgui_geom::Rect::new(zgui_geom::Point::new(0, 0), zgui_geom::Size::new(8, 8)),
        };
        let reference = paints.add(Paint::Image {
            tile,
            destination: zgui_geom::Rect::new(
                zgui_geom::Point::new(zgui_geom::DevicePx(0.0), zgui_geom::DevicePx(0.0)),
                zgui_geom::Size::new(zgui_geom::DevicePx(8.0), zgui_geom::DevicePx(8.0)),
            ),
            transform: zgui_scene::SpatialId::VIEWPORT,
            repeating: false,
        });
        assert!(of(reference, &paints).is_none());
    }
}
