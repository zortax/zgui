//! Repeated outlines drawn as glyph marks: the route for a scatter of triangles, crosses or stars.
//!
//! A part of a shape whose subpaths copy a few outlines becomes one [`MarkItem`] whose payload
//! holds a tile table and one anchor per subpath. Each outline is rasterised once per device scale
//! into an atlas sheet of 16 phase cells, and every copy draws the cell of its own device phase.
//! Copies two device pixels apart are painted one by one; copies that come closer are summed into
//! one coverage first, which is the union a path paints.
//!
//! The anchors stay in the shape's own units and the item maps them by the fit, so a canvas
//! view panned without a zoom keeps the payload, and a moved replay draws new phases from the
//! same bytes.

use std::sync::Arc;

use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_scene::{MarkFlags, MarkItem, MarkPayload, PaintRef, Scene, VectorId, kurbo, peniko};

use super::document::{density_of, reference, stroke_paint};
use super::recognise::separated_with;
use super::split::{self, Split, SplitDeclined, Winding};
use super::{ShapePaint, ShapeSource, VectorPlacement};
use crate::content::vectors::path_glyphs::{PHASES, PartPayload};
use crate::content::vectors::{GlyphRequest, GlyphSheets, VectorMaskSource, VectorMaskStyle};

/// How many path elements make a failed split worth remembering.
const REMEMBERED: usize = 64;

/// How far apart, in device pixels, copies are kept on each side to be painted one by one.
const MARGIN: f32 = 1.0;

/// One part of a shape: what it is drawn in and what paints it.
struct Part<'a> {
    /// The part's index in the split's payloads: 0 for the fill, 1 for the stroke.
    index: usize,
    /// How its coverage is produced.
    style: VectorMaskStyle<'a>,
    /// Device pixels per stroke unit.
    scale: f64,
    /// The fill rule of a fill, and `None` for a stroke.
    rule: Option<peniko::Fill>,
}

/// Emits a shape whose subpaths copy a few outlines as one glyph mark per part, or declines the
/// route.
///
/// Nothing is pushed unless both parts take the route.
pub(super) fn emit_path_glyphs(
    scene: &mut Scene,
    id: VectorId,
    source: &ShapeSource<'_>,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
    affine: Option<zgui_geom::Affine2>,
) -> Option<usize> {
    if !masks.path_glyphs(id) {
        return None;
    }
    let shape = source.shape;
    if !shape.clips.is_empty() {
        return None;
    }
    let solid = |paint: &zgui_svg::Paint, inherited: Color| match paint {
        zgui_svg::Paint::Solid(ink) => Some(ink.resolve(inherited).alpha() != 0.0),
        zgui_svg::Paint::Gradient(_) => None,
    };
    let fill = match &shape.fill {
        Some(fill) => solid(&fill.paint, paint.fill)?.then_some(fill.rule),
        None => None,
    };
    let inherited_stroke;
    let stroke: Option<(&kurbo::Stroke, bool)> = match &shape.stroke {
        Some(stroke) => solid(&stroke.paint, paint.stroke.unwrap_or(paint.fill))?
            .then_some((&stroke.style, true)),
        None => match paint.stroke {
            Some(color) if color.alpha() != 0.0 => {
                inherited_stroke = kurbo::Stroke::new(f64::from(paint.stroke_width));
                Some((&inherited_stroke, false))
            }
            _ => None,
        },
    };
    if fill.is_none() && stroke.is_none() {
        return None;
    }
    if stroke.is_some_and(|(style, _)| !style.dash_pattern.is_empty()) {
        return None;
    }
    let affine = affine?;
    let [fa, fb, fc, fd, fe, ff] = source.fit.as_coeffs();
    let [a, b, c, d] = [affine.a, affine.b, affine.c, affine.d].map(f64::from);
    let linear = [
        a * fa + c * fb,
        b * fa + d * fb,
        a * fc + c * fd,
        b * fc + d * fd,
    ];
    let determinant = linear[0] * linear[3] - linear[1] * linear[2];
    if !linear.iter().all(|value| value.is_finite()) || determinant.abs() <= 1.0e-12 {
        return None;
    }
    let [l0, l1, l2, l3] = linear.map(|value| value as f32);
    let density = density_of(
        &zgui_geom::Affine2::new(l0, l1, l2, l3, 0.0, 0.0),
        stroke.is_some(),
    )?;
    let mut parts: smallvec::SmallVec<[Part<'_>; 2]> = smallvec::SmallVec::new();
    if let Some(rule) = fill {
        parts.push(Part {
            index: 0,
            style: VectorMaskStyle::Fill(rule),
            scale: 1.0,
            rule: Some(rule),
        });
    }
    if let Some((style, own)) = stroke {
        // The shape's own stroke is in its units; the element's is in the fragment's, which the
        // transform alone scales.
        let scale = if own {
            f64::from(density[0])
        } else {
            f64::from(density_of(&affine, true)?[0])
        };
        parts.push(Part {
            index: 1,
            style: VectorMaskStyle::Stroke(style),
            scale,
            rule: None,
        });
    }

    let found = match split_of(&shape.path, linear, masks) {
        Ok(found) => found,
        Err(why) => {
            if why != SplitDeclined::Few && shape.path.elements().len() >= REMEMBERED {
                masks.path_glyphs_declined(id);
            }
            return None;
        }
    };

    // The device map of the whole shape, for the anchors.
    let place = |x: f64, y: f64| {
        let (x, y) = (fa * x + fc * y + fe, fb * x + fd * y + ff);
        [
            a * x + c * y + f64::from(affine.tx),
            b * x + d * y + f64::from(affine.ty),
        ]
    };
    let mut lowered: smallvec::SmallVec<[(Lowered, &Part<'_>); 2]> = smallvec::SmallVec::new();
    for part in &parts {
        let sheets = masks.glyph_sheets(GlyphRequest {
            geometries: &found.geometries,
            style: part.style,
            scale: part.scale,
        })?;
        lowered.push((
            lower(&shape.path, linear, &found, &sheets, part, &place, masks)?,
            part,
        ));
    }

    // The device bounds, taken back through the transform: what the marks paint in the
    // fragment's space.
    let inverse = affine.invert()?;
    let mut pushed = 0;
    for (part, which) in lowered {
        let [x0, y0, x1, y1] = part.ink;
        let device = Rect::new(
            Point::new(DevicePx(x0 as f32), DevicePx(y0 as f32)),
            Size::new(DevicePx((x1 - x0) as f32), DevicePx((y1 - y0) as f32)),
        );
        let bounds = inverse.transform_rect(device);
        let paint_ref = match which.rule {
            Some(_) => {
                let fill = shape.fill.as_ref().expect("a fill part has a fill");
                reference(scene, &fill.paint, paint.fill)
            }
            None => stroke_paint(scene, source, paint),
        };
        pushed += push(scene, part, bounds, paint_ref, source.fit, placement);
    }
    Some(pushed)
}

/// The split of `path` under `linear`, from the cache when it holds one.
///
/// A path with too few subpaths is answered without the cache: counting its moves costs less
/// than an entry.
fn split_of(
    path: &Arc<kurbo::BezPath>,
    linear: [f64; 4],
    masks: &dyn VectorMaskSource,
) -> Result<Arc<Split>, SplitDeclined> {
    if path.elements().len() < split::MIN_SUBPATHS {
        return Err(SplitDeclined::Few);
    }
    if let Some(mut splits) = masks.glyph_splits()
        && let Some(entry) = splits.lookup(path, linear)
    {
        return entry.outcome.clone();
    }
    let outcome = split::split(path, linear).map(Arc::new);
    if outcome != Err(SplitDeclined::Few)
        && let Some(mut splits) = masks.glyph_splits()
    {
        splits.insert(path, linear, outcome.clone());
    }
    outcome
}

/// One part's payload and flags, and what its copies paint on the device.
struct Lowered {
    /// The table and the anchors.
    payload: Arc<MarkPayload>,
    /// [`MarkFlags`] bits.
    flags: u32,
    /// How many words lead as the table.
    tiles: u32,
    /// The atlas texture the cells lie in.
    texture: u32,
    /// What the copies paint, as `[x0, y0, x1, y1]` on the device.
    ink: [f64; 4],
}

/// The payload of one part over `sheets`, from the split's cache while it was built from the same
/// sheets, or `None` for a union that would fill holes.
fn lower(
    path: &Arc<kurbo::BezPath>,
    linear: [f64; 4],
    found: &Split,
    sheets: &GlyphSheets,
    part: &Part<'_>,
    place: &dyn Fn(f64, f64) -> [f64; 2],
    masks: &dyn VectorMaskSource,
) -> Option<Lowered> {
    // Where each copy can put a pixel: the union of its cells from the pixel of its anchor, and
    // the pixel within seven eighths left of the anchor and an eighth right of it.
    let rect = |index: usize| {
        let [x, y] = found.anchors[index];
        let [x, y] = place(f64::from(x), f64::from(y));
        let reach = sheets.reach[usize::from(found.of[index])];
        [
            x + f64::from(reach[0]) - 0.875,
            y + f64::from(reach[1]) - 0.875,
            x + f64::from(reach[2]) + 0.125,
            y + f64::from(reach[3]) + 0.125,
        ]
    };
    let mut ink = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for index in 0..found.anchors.len() {
        let [x0, y0, x1, y1] = rect(index);
        ink = [
            ink[0].min(x0),
            ink[1].min(y0),
            ink[2].max(x1),
            ink[3].max(y1),
        ];
    }
    if !ink.iter().all(|value| value.is_finite()) {
        return None;
    }
    let tiles = (found.geometries.len() * PHASES) as u32;
    let held = masks.glyph_splits().and_then(|mut splits| {
        let entry = splits.lookup(path, linear)?;
        let held = entry.payloads[part.index].as_ref()?;
        (*held.keys == sheets.keys[..]).then(|| (Arc::clone(&held.payload), held.union))
    });
    let (payload, union) = match held {
        Some(held) => held,
        None => {
            let largest = sheets
                .reach
                .iter()
                .map(|reach| (reach[2] - reach[0]).max(reach[3] - reach[1]) as f32 + 1.0)
                .fold(0.0f32, f32::max);
            let union = !separated_with(found.anchors.len(), largest, MARGIN, |index| {
                rect(index).map(|value| value as f32)
            });
            let mut glyphs = Vec::with_capacity(sheets.table.len() + found.anchors.len());
            glyphs.extend_from_slice(&sheets.table);
            for (index, (&[x, y], &outline)) in found.anchors.iter().zip(&found.of).enumerate() {
                glyphs.push([
                    x.to_bits(),
                    y.to_bits(),
                    u32::from(outline),
                    tiles + index as u32,
                ]);
            }
            let payload = Arc::new(MarkPayload {
                glyphs,
                ..MarkPayload::default()
            });
            if let Some(mut splits) = masks.glyph_splits()
                && let Some(entry) = splits.lookup(path, linear)
            {
                entry.payloads[part.index] = Some(PartPayload {
                    keys: sheets.keys.clone().into_boxed_slice(),
                    union,
                    payload: Arc::clone(&payload),
                });
            }
            (payload, union)
        }
    };
    // The interior of overlapping copies is their union under the nonzero rule when every
    // outline winds one way everywhere. Winding both ways, or under the even-odd rule, an
    // overlap can be a hole, which no sum of coverage draws.
    if union {
        let holes = match part.rule {
            Some(peniko::Fill::EvenOdd) => true,
            Some(peniko::Fill::NonZero) => found
                .geometries
                .iter()
                .fold(Winding::default(), |held, geometry| {
                    held.union(geometry.winding)
                })
                .mixed(),
            None => false,
        };
        if holes {
            return None;
        }
    }
    Some(Lowered {
        payload,
        flags: if union { MarkFlags::UNION } else { 0 },
        tiles,
        texture: sheets.texture,
        ink,
    })
}

/// Pushes one part as one glyph mark, and returns how many survived.
fn push(
    scene: &mut Scene,
    lowered: Lowered,
    bounds: Rect<DevicePx, Device>,
    paint: PaintRef,
    fit: kurbo::Affine,
    placement: VectorPlacement,
) -> usize {
    let mut item = MarkItem::new(bounds, paint, lowered.payload.counts());
    let [a, b, c, d, e, f] = fit.as_coeffs();
    item.flags = lowered.flags;
    item.clip = placement.clip.0;
    item.transform = placement.transform.index();
    item.axes = [a as f32, b as f32, c as f32, d as f32];
    item.origin = [e as f32, f as f32];
    item.tiles = lowered.tiles;
    item.texture = lowered.texture;
    usize::from(scene.push_marks(item, lowered.payload).is_some())
}
