//! Canvas series drawn as marks: data in data space, markers and lines in CSS pixels.
//!
//! A part of a series is one union mark over a payload built once per data allocation. The item
//! maps the payload to the fragment's space by the matrix of the drawing, and the payload's
//! lengths are local units, so a change of the canvas view changes the item and keeps the payload.
//!
//! A path marker is drawn as glyphs: its outline is rasterised once per device scale into the cells
//! of one sheet, and the payload holds the sheet's table and one anchor per point. A marker the
//! glyph route cannot take draws through the general route, placed at every point.

use std::sync::Arc;

use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_profile::{Counter, counter};
use zgui_scene::kurbo::{self, Affine, BezPath};
use zgui_scene::{MarkFlags, MarkItem, PaintRef, Scene, VectorId, VectorItem, VectorStroke};

use super::document::{density_of, reference};
use super::marks::MAX_MARK_PRIMS;
use super::split::geometry_of;
use super::{ShapeEmission, ShapePaint, VectorPlacement, VectorRoute, under};
use crate::content::vectors::payloads::{SeriesPart, lod_bucket, series_payload};
use crate::content::vectors::{GlyphRequest, GlyphSheets, VectorMaskSource, VectorMaskStyle};

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

/// Emits one series as one union mark per part, and reports the marks route, or the path glyph
/// route for a path marker.
///
/// `fit` places canvas units in the fragment's space. A series whose matrix is not finite or has
/// no area draws nothing. Only a path marker takes another route, the general one, and `id` names
/// its items there.
///
/// With `lod`, a line whose points run left to right, with more than four points per device
/// column, is drawn through the first, lowest, highest and last point of each column.
#[expect(
    clippy::too_many_arguments,
    reason = "the series, where it goes, and its level of detail"
)]
pub(super) fn emit_series(
    scene: &mut Scene,
    id: VectorId,
    series: &zgui_canvas::Series,
    fit: Affine,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
    lod: bool,
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
            let (radius, square) = match marker {
                zgui_canvas::Marker::Circle { radius } => (*radius, false),
                zgui_canvas::Marker::Square { half } => (*half, true),
                zgui_canvas::Marker::Path(outline) => {
                    let mut parts: smallvec::SmallVec<[MarkerPart<'_>; 2]> =
                        smallvec::SmallVec::new();
                    if let Some(brush) = fill {
                        parts.push(MarkerPart {
                            width: None,
                            brush,
                            inherited: paint.fill,
                        });
                    }
                    if let Some((brush, width)) = stroke {
                        parts.push(MarkerPart {
                            width: Some(*width),
                            brush,
                            inherited: stroke_colour,
                        });
                    }
                    let markers = Markers {
                        outline,
                        data,
                        to_local,
                        fit,
                        parts: &parts,
                    };
                    return match emit_path_markers(scene, id, &markers, masks, placement) {
                        Some(emitted) => emitted,
                        None => emit_placed_markers(scene, id, &markers, placement),
                    };
                }
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
            let part = if lod {
                columns(scene, data, to_local, placement, masks)
                    .map_or(SeriesPart::Line, |bucket| SeriesPart::Columns { bucket })
            } else {
                SeriesPart::Line
            };
            parts.push(Part {
                part,
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
                &[],
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

/// The column bucket a line over `data` is reduced at, or `None` when it is drawn whole.
///
/// The data must run left to right, and the line must map data x to device x alone, under an
/// upright transform. `None` when a device column holds four points or fewer.
fn columns(
    scene: &Scene,
    data: &Arc<[[f32; 2]]>,
    to_local: Affine,
    placement: VectorPlacement,
    masks: &dyn VectorMaskSource,
) -> Option<i32> {
    let [a, b, c, d, _, _] = to_local.as_coeffs();
    let upright = |b: f64, c: f64, scale: f64| b.abs() <= 1e-9 * scale && c.abs() <= 1e-9 * scale;
    if !upright(b, c, a.abs().max(d.abs())) {
        return None;
    }
    let spatial = scene
        .spatial
        .resolve(placement.transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2)?;
    let (sa, sb, sc, sd) = (
        f64::from(spatial.a),
        f64::from(spatial.b),
        f64::from(spatial.c),
        f64::from(spatial.d),
    );
    if !upright(sb, sc, sa.abs().max(sd.abs())) {
        return None;
    }
    let per_unit = (a * sa).abs();
    let mut cache = masks.payloads();
    lod_bucket(cache.as_deref_mut(), data, per_unit)
}

/// One part of a path marker: its fill, or its stroke of a width in CSS pixels.
struct MarkerPart<'a> {
    /// The stroke width, or `None` for the fill.
    width: Option<f64>,
    /// What paints the part.
    brush: &'a zgui_canvas::Brush,
    /// The colour an inherited brush takes.
    inherited: zgui_color::Color,
}

/// A path marker series, as both of its routes read it.
struct Markers<'a> {
    /// The marker, in CSS pixels about its origin.
    outline: &'a Arc<BezPath>,
    /// The points, in data space.
    data: &'a Arc<[[f32; 2]]>,
    /// Data space to the fragment's space.
    to_local: Affine,
    /// Canvas units to the fragment's space, which places a ramp.
    fit: Affine,
    /// The fill, then the stroke.
    parts: &'a [MarkerPart<'a>],
}

/// Emits a path marker series as one union glyph mark per part, or `None` when the glyph route
/// cannot take it: a transform that turns or shears, a marker over 64 device pixels across, a
/// filled marker that winds both ways, or no sheet.
fn emit_path_markers(
    scene: &mut Scene,
    id: VectorId,
    markers: &Markers<'_>,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> Option<ShapeEmission> {
    if !masks.path_glyphs(id) {
        return None;
    }
    let affine = scene
        .spatial
        .resolve(placement.transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2)?;
    let scale = placement.scale;
    // A marker is in CSS pixels, which the scale and the transform alone take to the device.
    let linear = [affine.a, affine.b, affine.c, affine.d].map(|value| f64::from(value * scale));
    let stroked = markers.parts.iter().any(|part| part.width.is_some());
    let [l0, l1, l2, l3] = linear.map(|value| value as f32);
    let density = density_of(&zgui_geom::Affine2::new(l0, l1, l2, l3, 0.0, 0.0), stroked)?;
    let spatial = density_of(&affine, false)?;
    let geometry = geometry_of(markers.outline, linear).ok()?;
    // Overlapping copies are summed into one coverage, which is their union under the nonzero rule
    // only while the marker winds one way everywhere.
    if geometry.winding.mixed() && markers.parts.iter().any(|part| part.width.is_none()) {
        return None;
    }
    let mut drawn: smallvec::SmallVec<[(GlyphSheets, &MarkerPart<'_>); 2]> =
        smallvec::SmallVec::new();
    for part in markers.parts {
        let stroke = part.width.map(kurbo::Stroke::new);
        let style = match &stroke {
            Some(stroke) => VectorMaskStyle::Stroke(stroke),
            None => VectorMaskStyle::Fill(zgui_scene::peniko::Fill::NonZero),
        };
        let sheets = masks.glyph_sheets(GlyphRequest {
            geometries: core::slice::from_ref(&geometry),
            style,
            scale: f64::from(density[0]),
        })?;
        drawn.push((sheets, part));
    }

    let [a, b, c, d, _, _] = markers.to_local.as_coeffs();
    let mut pushed = 0;
    let mut drew = false;
    for (sheets, part) in drawn {
        let reach = sheets.reach[0];
        let sheet = sheets.keys[0].handle();
        let built = {
            let mut cache = masks.payloads();
            series_payload(
                cache.as_deref_mut(),
                markers.data,
                SeriesPart::Glyph { sheet },
                markers.to_local,
                MAX_MARK_PRIMS,
                &sheets.table,
            )
        };
        let Some(built) = built else {
            continue;
        };
        // Every copy lies within its cells from the pixel of its anchor, and that pixel within a
        // pixel of the anchor.
        let farthest = reach
            .iter()
            .map(|edge| edge.unsigned_abs())
            .max()
            .unwrap_or(0) as f32;
        let margin = f64::from((farthest + 1.0) / spatial[0].min(spatial[1]));
        let origin = markers.to_local * kurbo::Point::new(built.centre[0], built.centre[1]);
        let [x0, y0, x1, y1] = built.bounds;
        let ink = markers
            .to_local
            .transform_rect_bbox(kurbo::Rect::new(x0, y0, x1, y1))
            .inflate(margin, margin);
        let bounds = Rect::new(
            Point::new(DevicePx(ink.x0 as f32), DevicePx(ink.y0 as f32)),
            Size::new(DevicePx(ink.width() as f32), DevicePx(ink.height() as f32)),
        );
        let paint_ref = brush_paint(scene, part.brush, markers.fit, part.inherited);
        for payload in built.payloads {
            let mut item = MarkItem::new(bounds, paint_ref, payload.counts());
            item.flags = MarkFlags::UNION | MarkFlags::SCREEN;
            item.clip = placement.clip.0;
            item.transform = placement.transform.index();
            item.axes = [a as f32, b as f32, c as f32, d as f32];
            item.origin = [origin.x as f32, origin.y as f32];
            item.tiles = sheets.table.len() as u32;
            item.texture = sheets.texture;
            pushed += usize::from(scene.push_marks(item, payload).is_some());
            drew = true;
        }
    }
    if !drew {
        return Some(ShapeEmission::default());
    }
    counter::bump(Counter::VectorRoutePathGlyphs);
    Some(ShapeEmission {
        pushed,
        route: Some(VectorRoute::PathGlyphs),
    })
}

/// Emits a path marker series through the general route: one item per part, the marker placed
/// at every finite point in the fragment's space.
fn emit_placed_markers(
    scene: &mut Scene,
    id: VectorId,
    markers: &Markers<'_>,
    placement: VectorPlacement,
) -> ShapeEmission {
    let scale = f64::from(placement.scale);
    let mut path = BezPath::new();
    for &[x, y] in markers.data.iter() {
        if !(x.is_finite() && y.is_finite()) {
            continue;
        }
        let at = markers.to_local * kurbo::Point::new(f64::from(x), f64::from(y));
        let place = Affine::translate(at.to_vec2()) * Affine::scale(scale);
        path.extend(
            markers
                .outline
                .elements()
                .iter()
                .map(|element| place * *element),
        );
    }
    if path.is_empty() {
        return ShapeEmission::default();
    }
    let path = Arc::new(path);
    let bounds = kurbo::Shape::bounding_box(&*path);
    let mut pushed = 0;
    for part in markers.parts {
        let paint_ref = brush_paint(scene, part.brush, markers.fit, part.inherited);
        let mut item = match part.width {
            None => VectorItem::filled(id, Arc::clone(&path), paint_ref),
            Some(width) => VectorItem::styled(
                id,
                Arc::clone(&path),
                VectorStroke {
                    paint: paint_ref,
                    style: kurbo::Stroke::new(width * scale),
                },
            ),
        };
        let reach = item
            .stroke
            .as_ref()
            .map_or(0.0, |stroke| f64::from(stroke.reach()));
        let local = rect_of(bounds.inflate(reach, reach));
        item.ink = under(scene, placement.transform, local);
        item.local_ink = local;
        item.transform = Some(placement.transform);
        let item = item.clipped(placement.clip);
        pushed += usize::from(scene.push_vector(item).is_some());
    }
    counter::bump(Counter::VectorRouteGeneral);
    ShapeEmission {
        pushed,
        route: Some(VectorRoute::GeneralRaster),
    }
}

/// A kurbo rectangle in the geometry the display list is written in.
fn rect_of(rect: kurbo::Rect) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(rect.x0 as f32), DevicePx(rect.y0 as f32)),
        Size::new(
            DevicePx(rect.width() as f32),
            DevicePx(rect.height() as f32),
        ),
    )
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
