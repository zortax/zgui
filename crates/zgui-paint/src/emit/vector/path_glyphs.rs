//! Repeated outlines drawn as placements: the route for a scatter of triangles, crosses or stars.
//!
//! A part of a shape whose subpaths copy a few outlines becomes one [`Marks`] item whose payload
//! holds each outline once, in device pixels, and one placement per subpath. The compositor
//! rasterises each outline and draws every copy at its own device phase. Copies two device pixels
//! apart are painted one by one, and copies that come closer are summed into one coverage first,
//! which is the union a path paints.
//!
//! The anchors stay in the shape's own units and the item maps them by the fit, so a canvas view
//! panned without a zoom keeps the payload.
//!
//! [`Marks`]: compositor_types::Marks

use std::sync::Arc;

use crate::VectorId;
use compositor_types::{MarkFlags, MarkPayload, MarkPlacement};
use zgui_color::Color;
use zgui_geom::{DevicePx, Point, Rect, Size};

use super::document::{density_of, reference, stroke_paint};
use super::recognise::separated_with;
use super::split::{self, Split, SplitDeclined, Winding};
use super::{ShapePaint, ShapeSource, VectorPlacement};
use crate::content::vectors::VectorMaskSource;
use crate::content::vectors::path_glyphs::{PartPayload, PartStyle, outlines};
use crate::emit::record::Recorder;

/// How many path elements make a failed split worth remembering.
const REMEMBERED: usize = 64;

/// How far apart, in device pixels, copies are kept on each side to be painted one by one.
const MARGIN: f32 = 1.0;

/// One part of a shape: what it is drawn in and what paints it.
struct Part<'a> {
    /// The part's index in the split's payloads: 0 for the fill, 1 for the stroke.
    index: usize,
    /// How its outlines are produced.
    style: PartStyle<'a>,
    /// The fill rule of a fill, and `None` for a stroke.
    rule: Option<peniko::Fill>,
}

/// Records a shape whose subpaths copy a few outlines as one marks item per part, or declines the
/// route.
///
/// Nothing is recorded unless both parts take the route. A fill under the even-odd rule declines,
/// since placements are filled under the nonzero rule.
pub(super) fn emit_path_glyphs(
    rec: &mut Recorder<'_>,
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
    if fill == Some(peniko::Fill::EvenOdd) {
        return None;
    }
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
            style: PartStyle::Fill,
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
            style: PartStyle::Stroke { style, scale },
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
        lowered.push((
            lower(&shape.path, linear, &found, part, &place, masks)?,
            part,
        ));
    }

    // The device bounds, taken back through the transform: what the marks paint in the
    // fragment's space.
    let inverse = affine.invert()?;
    let clip = rec.clip(placement.clip);
    let space = rec.space(placement.transform);
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
                reference(rec.tables(), &fill.paint, paint.fill)
            }
            None => stroke_paint(rec.tables(), source, paint),
        };
        let paint_ref = rec.paint(paint_ref);
        let mut item = super::marks::item(bounds, paint_ref, &part.payload, (clip, space));
        let [a, b, c, d, e, f] = source.fit.as_coeffs();
        item.flags = part.flags;
        item.axes = [a as f32, b as f32, c as f32, d as f32];
        item.origin = [e as f32, f as f32];
        rec.marks(item, part.payload);
        pushed += 1;
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
    /// The outlines and the anchors.
    payload: Arc<MarkPayload>,
    /// How the copies are combined.
    flags: MarkFlags,
    /// What the copies paint, as `[x0, y0, x1, y1]` on the device.
    ink: [f64; 4],
}

/// The payload of one part, from the split's cache while it was built in the same style, or
/// `None` for outlines past the size cap or a union that would fill holes.
fn lower(
    path: &Arc<kurbo::BezPath>,
    linear: [f64; 4],
    found: &Split,
    part: &Part<'_>,
    place: &dyn Fn(f64, f64) -> [f64; 2],
    masks: &dyn VectorMaskSource,
) -> Option<Lowered> {
    let style = part.style.key();
    let held = masks.glyph_splits().and_then(|mut splits| {
        let entry = splits.held(path, linear)?;
        let held = entry.payloads[part.index].as_ref()?;
        (held.style == style).then(|| {
            (
                Arc::clone(&held.payload),
                held.union,
                Arc::clone(&held.reach),
            )
        })
    });
    let (cached, shapes, reach) = match held {
        Some((payload, union, reach)) => (Some((payload, union)), Vec::new(), reach),
        None => {
            let built = outlines(&found.geometries, part.style)?;
            (None, built.shapes, built.reach)
        }
    };
    // Where each copy can put a pixel: its outline from its anchor, and a pixel on each side for
    // the quantised anchor and the whole-pixel raster.
    let rect = |index: usize| {
        let [x, y] = found.anchors[index];
        let [x, y] = place(f64::from(x), f64::from(y));
        let [x0, y0, x1, y1] = reach[usize::from(found.of[index])];
        [x + x0 - 1.0, y + y0 - 1.0, x + x1 + 1.0, y + y1 + 1.0]
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
    let (payload, union) = match cached {
        Some(cached) => cached,
        None => {
            let largest = reach
                .iter()
                .map(|[x0, y0, x1, y1]| (x1 - x0).max(y1 - y0) as f32 + 2.0)
                .fold(0.0f32, f32::max);
            let union = !separated_with(found.anchors.len(), largest, MARGIN, |index| {
                rect(index).map(|value| value as f32)
            });
            let placements = found
                .anchors
                .iter()
                .zip(&found.of)
                .map(|(&[x, y], &outline)| MarkPlacement {
                    x,
                    y,
                    shape: u32::from(outline),
                    _pad: 0,
                })
                .collect();
            let payload = Arc::new(MarkPayload {
                shapes,
                placements,
                ..MarkPayload::default()
            });
            if let Some(mut splits) = masks.glyph_splits()
                && let Some(entry) = splits.held(path, linear)
            {
                entry.payloads[part.index] = Some(PartPayload {
                    style,
                    union,
                    reach: Arc::clone(&reach),
                    payload: Arc::clone(&payload),
                });
            }
            (payload, union)
        }
    };
    // The interior of overlapping copies is their union under the nonzero rule when every
    // outline winds one way everywhere. Winding both ways, an overlap can be a hole, which no
    // sum of coverage draws. A stroke's outline is always the union of its segments'.
    if union && part.rule.is_some() {
        let holes = found
            .geometries
            .iter()
            .fold(Winding::default(), |held, geometry| {
                held.union(geometry.winding)
            })
            .mixed();
        if holes {
            return None;
        }
    }
    Some(Lowered {
        payload,
        flags: if union {
            MarkFlags::UNION
        } else {
            MarkFlags::default()
        },
        ink,
    })
}

#[cfg(test)]
mod tests;
