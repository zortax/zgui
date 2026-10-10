//! Canvas series drawn as marks: data in data space, markers and lines in CSS pixels.
//!
//! A part of a series is one union mark over a payload built once per data allocation. The item
//! maps the payload to the fragment's space by the matrix of the drawing, and the payload's
//! lengths are local units, so a change of the canvas view changes the item and keeps the payload.

use zgui_profile::{Counter, counter};
use zgui_scene::kurbo::{self, Affine};
use zgui_scene::{MarkFlags, MarkItem, PaintRef, Scene};

use super::document::reference;
use super::marks::MAX_MARK_PRIMS;
use super::{ShapeEmission, ShapePaint, VectorPlacement, VectorRoute};
use crate::content::vectors::VectorMaskSource;
use crate::content::vectors::payloads::{SeriesPart, series_payload};

/// One part of a series before its payload is found.
struct Part<'a> {
    /// What the payload is keyed and built by.
    part: SeriesPart,
    /// How far the part reaches past a point, in local units.
    reach: f64,
    /// Half the line width in local units, or zero.
    half_width: f32,
    /// [`MarkFlags`] bits besides the union and the screen bits.
    flags: u32,
    /// What paints the part.
    brush: &'a zgui_canvas::Brush,
    /// The colour an inherited brush takes.
    inherited: zgui_color::Color,
}

/// Emits one series as one union mark per part, and reports the marks route.
///
/// `fit` places canvas units in the fragment's space. A series whose matrix is not finite or has
/// no area draws nothing. Series take no other route.
pub(super) fn emit_series(
    scene: &mut Scene,
    series: &zgui_canvas::Series,
    fit: Affine,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> ShapeEmission {
    let (data, to_canvas) = match series {
        zgui_canvas::Series::Points {
            data, to_canvas, ..
        }
        | zgui_canvas::Series::Line {
            data, to_canvas, ..
        } => (data, *to_canvas),
    };
    let to_local = fit * to_canvas;
    let coefficients = to_local.as_coeffs();
    if !coefficients.iter().all(|value| value.is_finite()) || to_local.determinant().abs() < 1e-12 {
        return ShapeEmission::default();
    }
    let scale = f64::from(placement.scale);
    let stroke_colour = paint.stroke.unwrap_or(paint.fill);
    let mut parts: smallvec::SmallVec<[Part<'_>; 2]> = smallvec::SmallVec::new();
    match series {
        zgui_canvas::Series::Points {
            marker,
            fill,
            stroke,
            ..
        } => {
            let (radius, square) = match *marker {
                zgui_canvas::Marker::Circle { radius } => (radius, false),
                zgui_canvas::Marker::Square { half } => (half, true),
                // A marker this lowering does not know draws nothing.
                _ => return ShapeEmission::default(),
            };
            let flags = if square { MarkFlags::SQUARE_DISCS } else { 0 };
            let disc = |outer: f64, inner: f64| SeriesPart::Disc {
                outer: (outer as f32).to_bits(),
                inner: (inner as f32).to_bits(),
                square,
            };
            if let Some(brush) = fill {
                let outer = radius * scale;
                parts.push(Part {
                    part: disc(outer, 0.0),
                    reach: outer,
                    half_width: 0.0,
                    flags,
                    brush,
                    inherited: paint.fill,
                });
            }
            if let Some((brush, width)) = stroke {
                let outer = (radius + width / 2.0) * scale;
                let inner = (radius - width / 2.0).max(0.0) * scale;
                parts.push(Part {
                    part: disc(outer, inner),
                    reach: outer,
                    half_width: 0.0,
                    flags,
                    brush,
                    inherited: stroke_colour,
                });
            }
        }
        zgui_canvas::Series::Line { stroke, brush, .. } => {
            let half = stroke.width / 2.0 * scale;
            parts.push(Part {
                part: SeriesPart::Line,
                reach: half * core::f64::consts::SQRT_2,
                half_width: half as f32,
                flags: MarkFlags::caps(cap(stroke.start_cap), cap(stroke.end_cap)),
                brush,
                inherited: stroke_colour,
            });
        }
    }

    let [a, b, c, d, _, _] = coefficients;
    let mut pushed = 0;
    let mut drew = false;
    for part in parts {
        let payloads = {
            let mut cache = masks.payloads();
            series_payload(
                cache.as_deref_mut(),
                data,
                part.part,
                to_local,
                MAX_MARK_PRIMS,
            )
        };
        let Some(built) = payloads else {
            continue;
        };
        let origin = to_local * kurbo::Point::new(built.centre[0], built.centre[1]);
        let [x0, y0, x1, y1] = built.bounds;
        let ink = to_local
            .transform_rect_bbox(kurbo::Rect::new(x0, y0, x1, y1))
            .inflate(part.reach, part.reach);
        let bounds = zgui_geom::Rect::new(
            zgui_geom::Point::new(
                zgui_geom::DevicePx(ink.x0 as f32),
                zgui_geom::DevicePx(ink.y0 as f32),
            ),
            zgui_geom::Size::new(
                zgui_geom::DevicePx(ink.width() as f32),
                zgui_geom::DevicePx(ink.height() as f32),
            ),
        );
        let paint_ref = brush_paint(scene, part.brush, fit, part.inherited);
        for payload in built.payloads {
            let mut item = MarkItem::new(bounds, paint_ref, payload.counts());
            item.flags = MarkFlags::UNION | MarkFlags::SCREEN | part.flags;
            item.clip = placement.clip.0;
            item.transform = placement.transform.index();
            item.half_width = part.half_width;
            item.axes = [a as f32, b as f32, c as f32, d as f32];
            item.origin = [origin.x as f32, origin.y as f32];
            pushed += usize::from(scene.push_marks(item, payload).is_some());
            drew = true;
        }
    }
    if !drew {
        return ShapeEmission::default();
    }
    counter::bump(Counter::VectorRouteMarks);
    ShapeEmission {
        pushed,
        route: Some(VectorRoute::Marks),
    }
}

/// The interned paint of `brush`, placed by `fit`. A colour does not move; a ramp does.
fn brush_paint(
    scene: &mut Scene,
    brush: &zgui_canvas::Brush,
    fit: Affine,
    inherited: zgui_color::Color,
) -> PaintRef {
    let paint = brush.paint();
    let paint = if fit == Affine::IDENTITY {
        paint
    } else {
        zgui_svg::document::place::paint(&paint, fit)
    };
    reference(scene, &paint, inherited)
}

/// The [`MarkFlags`] cap of a stroke cap.
fn cap(cap: kurbo::Cap) -> u32 {
    match cap {
        kurbo::Cap::Butt => MarkFlags::BUTT,
        kurbo::Cap::Square => MarkFlags::SQUARE,
        kurbo::Cap::Round => MarkFlags::ROUND,
    }
}
