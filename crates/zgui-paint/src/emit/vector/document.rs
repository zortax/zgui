//! One shape of a drawing, as the primitives that draw it.
//!
//! # Where a shape's colour comes from
//!
//! A shape names its own paint or asks for the inherited one, and both arrive through the same
//! element. A document that wrote `currentColor` — and a bare list of outlines, which is the same
//! thing with nothing else in it — asks for the inherited one, and gets the element's, so one icon
//! file is the colour of the text beside it in every context it is used. A document that wrote a
//! colour keeps it, and is not tinted by anything: a two-colour logo inside a paragraph of red text
//! is still a two-colour logo.
//!
//! What "the inherited one" resolves to is [`ShapePaint`], which is the element's `--zgui-fill` if
//! the cascade produced one and its computed `color` otherwise. That is one knob and not two: an
//! author who overrides what a drawing is filled with overrides what its `currentColor` means, and
//! anything else would give two answers to one question.

use std::sync::Arc;

use iceblink::{FillRule, PathClip, PathFill, PathItem, PathStroke};
use zgui_color::{Color, ColorSpace, HueInterpolation};
use zgui_profile::{Counter, counter};

use crate::paints::{GradientKind, Paint, PaintRef};

use crate::content::vectors::VectorMaskSource;
use crate::emit::record::Recorder;
use crate::emit::vector::{ShapePaint, ShapeSource, VectorPlacement};

use super::analytic::emit_analytic;
use super::marks::emit_marks;
use super::path_glyphs::emit_path_glyphs;
use super::{ShapeEmission, VectorRoute};

/// Records one shape, and returns how many items were recorded.
pub fn emit(
    rec: &mut Recorder<'_>,
    id: crate::VectorId,
    shape: &zgui_svg::Shape,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> usize {
    emit_tracked(
        rec,
        id,
        &ShapeSource::placed_shape(shape),
        paint,
        masks,
        placement,
    )
    .pushed
}

/// Records one shape and reports which route it selected.
///
/// Every route reads the source shape in its own space. The general route records the source path
/// with the fit as the item's placement, so a new fit makes no new path.
pub(crate) fn emit_tracked(
    rec: &mut Recorder<'_>,
    id: crate::VectorId,
    source: &ShapeSource<'_>,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> ShapeEmission {
    // Resolved once for both recognising routes, which read the same matrix.
    let affine = rec
        .tables_ref()
        .spatial
        .resolve(placement.transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2);
    if let Some(pushed) = emit_analytic(rec, id, source, paint, masks, placement, affine, false) {
        counter::bump(Counter::VectorRouteAnalytic);
        return ShapeEmission {
            pushed,
            route: Some(VectorRoute::Analytic),
        };
    }
    if let Some(pushed) = emit_marks(rec, id, source, paint, masks, placement, affine, false) {
        counter::bump(Counter::VectorRouteMarks);
        return ShapeEmission {
            pushed,
            route: Some(VectorRoute::Marks),
        };
    }
    if let Some(pushed) = emit_path_glyphs(rec, id, source, paint, masks, placement, affine) {
        counter::bump(Counter::VectorRoutePathGlyphs);
        return ShapeEmission {
            pushed,
            route: Some(VectorRoute::PathGlyphs),
        };
    }
    // A fit with no area is a box of no size, and nothing drawn into it reaches a pixel.
    let area = source.fit.determinant();
    if !area.is_finite() || area.abs() <= 1.0e-12 {
        return ShapeEmission::default();
    }
    // A transform with no area behind it, such as `scale(0)` or a projection flattened to one,
    // takes every outline to a line or a point, so nothing it draws can reach a pixel.
    if flattened(affine) {
        return ShapeEmission::default();
    }
    let shape = source.shape;
    let stroke = stroke_of(rec.tables(), shape, paint, source.fit);
    // A part whose paint paints nothing is left out, so the other part is drawn as it would be
    // alone.
    let fill = shape
        .fill
        .as_ref()
        .map(|fill| {
            let reference = reference(rec.tables(), &fill.paint, paint.fill);
            PathFill {
                paint: rec.paint(reference),
                rule: fill_rule(fill.rule),
            }
        })
        .filter(|fill| fill.paint != iceblink::PaintRef::NONE);
    let stroke = stroke
        .map(|(stroke, style)| PathStroke {
            paint: rec.paint(stroke),
            style,
        })
        .filter(|stroke| stroke.paint != iceblink::PaintRef::NONE);
    if fill.is_none() && stroke.is_none() {
        return ShapeEmission::default();
    }
    let item = PathItem {
        path: Arc::clone(&shape.path),
        placement: source.fit,
        brush: kurbo::Affine::IDENTITY,
        fill,
        stroke,
        clips: path_clips(&shape.clips),
        clip: rec.clip(placement.clip),
        space: rec.space(placement.transform),
    };
    if !rec.path(item) {
        return ShapeEmission::default();
    }
    counter::bump(Counter::VectorRouteGeneral);
    ShapeEmission {
        pushed: 1,
        route: Some(VectorRoute::GeneralRaster),
    }
}

/// Returns the compositor's fill rule for `rule`.
pub(super) fn fill_rule(rule: peniko::Fill) -> FillRule {
    match rule {
        peniko::Fill::NonZero => FillRule::NON_ZERO,
        peniko::Fill::EvenOdd => FillRule::EVEN_ODD,
    }
}

/// Returns a shape's own clips as the outlines a path item is cut to.
///
/// Every shape without clips shares one empty list, so recording it again compares equal.
pub(super) fn path_clips(clips: &[zgui_svg::Clip]) -> Arc<[PathClip]> {
    static NONE: std::sync::OnceLock<Arc<[PathClip]>> = std::sync::OnceLock::new();
    if clips.is_empty() {
        return Arc::clone(NONE.get_or_init(|| Arc::from([])));
    }
    clips
        .iter()
        .map(|clip| PathClip {
            path: Arc::clone(&clip.path),
            rule: fill_rule(clip.rule),
        })
        .collect()
}

/// How much residual rotation a linear part may carry and still be read as axis-preserving.
///
/// Relative, because the residue a matrix carries is proportional to the lengths in it: an absolute
/// bound refuses a quarter turn composed through three nested transforms as readily as it refuses a
/// real one.
const EPSILON: f32 = 1.0e-4;

/// Whether a transform takes every outline under it to something with no area.
fn flattened(affine: Option<zgui_geom::Affine2>) -> bool {
    let Some(affine) = affine else {
        return false;
    };
    let area = affine.a * affine.d - affine.b * affine.c;
    !area.is_finite() || area.abs() <= f32::EPSILON
}

/// Returns the device pixels per unit of each of a shape's own axes under `affine`.
///
/// Returns `None` unless `affine` keeps the axes or turns them by quarter turns, and for a
/// `stroked` shape unless it also scales both axes alike.
pub(super) fn density_of(affine: &zgui_geom::Affine2, stroked: bool) -> Option<[f32; 2]> {
    let kx = affine.a.hypot(affine.b);
    let ky = affine.c.hypot(affine.d);
    const MIN_DENSITY: f32 = 1.0e-4;
    if !kx.is_finite() || !ky.is_finite() || kx < MIN_DENSITY || ky < MIN_DENSITY {
        return None;
    }
    let axial = affine.b.abs() <= EPSILON * kx && affine.c.abs() <= EPSILON * ky;
    let quarter = affine.a.abs() <= EPSILON * kx && affine.d.abs() <= EPSILON * ky;
    if !axial && !quarter {
        return None;
    }
    // A stroke is measured along the outline, so a map that scales the two axes differently gives
    // it two widths.
    if stroked && (kx - ky).abs() > EPSILON * kx.max(ky) {
        return None;
    }
    Some([kx, ky])
}

/// Returns the paint and style that stroke one shape in its own space, from the shape's own stroke
/// or the element's.
///
/// A shape that named no stroke is stroked only when the element asked for one through
/// `--zgui-stroke`. That is what makes a bare outline strokeable from a stylesheet without giving
/// every shape of a vector document a stroke it never asked for. The element's width is in device
/// pixels, so it is divided by the scale `fit` applies; a fit with no scale strokes nothing.
pub(super) fn stroke_of(
    tables: &mut crate::Tables,
    shape: &zgui_svg::Shape,
    paint: &ShapePaint,
    fit: kurbo::Affine,
) -> Option<(PaintRef, kurbo::Stroke)> {
    match &shape.stroke {
        Some(stroke) => Some((
            reference(tables, &stroke.paint, paint.stroke.unwrap_or(paint.fill)),
            stroke.style.clone(),
        )),
        None => {
            let color = paint.stroke?;
            let width =
                f64::from(paint.stroke_width) / zgui_svg::document::place::uniform_scale(fit);
            if !width.is_finite() {
                return None;
            }
            Some((
                PaintRef::solid(tables.paints.solid(color)),
                kurbo::Stroke::new(width),
            ))
        }
    }
}

/// The paint the stroke of `source` is drawn with, placed in the fragment's space: the shape's
/// own, or the element's.
pub(super) fn stroke_paint(
    tables: &mut crate::Tables,
    source: &ShapeSource<'_>,
    paint: &ShapePaint,
) -> PaintRef {
    match &source.shape.stroke {
        Some(stroke) => reference(
            tables,
            &super::recognised::placed_paint(source, &stroke.paint),
            paint.stroke.unwrap_or(paint.fill),
        ),
        None => paint.stroke.map_or(PaintRef::NONE, |color| {
            PaintRef::solid(tables.paints.solid(color))
        }),
    }
}

/// The interned paint one of a document's paints becomes, given the inherited colour.
pub(super) fn reference(
    tables: &mut crate::Tables,
    paint: &zgui_svg::Paint,
    inherited: Color,
) -> PaintRef {
    match paint {
        zgui_svg::Paint::Solid(ink) => PaintRef::solid(tables.paints.solid(ink.resolve(inherited))),
        zgui_svg::Paint::Gradient(ramp) => {
            let kind = match ramp.kind {
                zgui_svg::GradientKind::Linear { start, end } => GradientKind::Linear {
                    start: point(start),
                    end: point(end),
                },
                zgui_svg::GradientKind::Radial {
                    center,
                    radius_x,
                    radius_y,
                } => GradientKind::Radial {
                    center: point(center),
                    radius_x: radius_x as f32,
                    radius_y: radius_y as f32,
                },
            };
            // In sRGB, with no densifying: a vector document's ramps are defined to interpolate in
            // sRGB, which is the space the rasteriser interpolates in, so the straight lines it
            // draws between the stops are the curve the document asked for.
            tables.paints.add(Paint::Gradient {
                kind,
                stops: ramp
                    .stops
                    .iter()
                    .map(|stop| {
                        zgui_color::GradientStop::new(stop.offset, stop.color.resolve(inherited))
                    })
                    .collect(),
                space: ColorSpace::Srgb,
                hue: HueInterpolation::Shorter,
                repeating: ramp.repeating,
            })
        }
    }
}

/// One of the document's points, in the geometry the display list is written in.
fn point(point: kurbo::Point) -> zgui_geom::Point<zgui_geom::DevicePx, zgui_geom::Device> {
    zgui_geom::Point::new(
        zgui_geom::DevicePx(point.x as f32),
        zgui_geom::DevicePx(point.y as f32),
    )
}
