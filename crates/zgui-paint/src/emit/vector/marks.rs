//! Recognised shapes drawn as marks: the route for many prims, and for prims that overlap.
//!
//! Each part of a shape whose subpaths are all circles, axis-aligned ellipses, rectangles, rounded
//! rectangles or simple strokes becomes one [`MarkItem`] over a shared payload of its prims. Prims
//! two device pixels apart are painted one by one. Prims that come closer are summed into one
//! coverage first and painted through the sum once, which is the union a path paints.
//!
//! The payload stays in the units the shape was recognised in, and the item maps it to the
//! fragment's space. So a payload lowered from a recognition the cache holds is the same
//! allocation under every fit, and a pan of a canvas view uploads none.

use std::sync::Arc;

use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_scene::{
    MarkBox, MarkFlags, MarkItem, MarkPayload, PaintRef, Scene, VectorId, kurbo, peniko,
};

use super::analytic::{clip_links, look, of_color};
use super::document::{reference, stroke_paint};
use super::recognise::{self, Decomposition, Orientation};
use super::recognised::{Outline, PartOf, placed_paint, recognised_in_source, straight_on_axes};
use super::{ShapePaint, ShapeSource, VectorPlacement};
use crate::content::vectors::VectorMaskSource;

/// The most prims one part of a shape may become.
pub(crate) const MAX_MARK_PRIMS: usize = 1 << 22;

/// The most path elements one prim is written with, as on the analytic route.
const ELEMENTS_PER_PRIM: usize = 72;

/// How many path elements make a failed recognition worth remembering.
const REMEMBERED: usize = 64;

/// How far apart, in device pixels, prims are kept on each side to be painted one by one.
const MARGIN: f32 = 1.0;

/// How many times over the discs of a union item may cover its ink before identical ones are
/// dropped.
const OVERDRAW: f32 = 8.0;

/// One part's payload and flags, before its paint is interned.
struct Lowered {
    /// The prims, in the units they were recognised in.
    payload: Arc<MarkPayload>,
    /// [`MarkFlags`] bits.
    flags: u32,
    /// What the prims paint together, in the fragment's space.
    ink: [f32; 4],
    /// Half the polyline stroke width, in the units the prims were recognised in.
    half_width: f32,
}

/// Emits a shape whose parts are recognised prims as one mark per part, or declines the route.
///
/// Nothing is pushed unless every part and every clip is recognised.
pub(super) fn emit_marks(
    scene: &mut Scene,
    id: VectorId,
    source: &ShapeSource<'_>,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
    affine: Option<zgui_geom::Affine2>,
) -> Option<usize> {
    if !masks.marks(id) {
        return None;
    }
    let shape = source.shape;
    let elements = shape.path.elements().len();
    // Too long to be written as the most prims a part may become.
    if elements > MAX_MARK_PRIMS * ELEMENTS_PER_PRIM {
        masks.marks_declined(id);
        return None;
    }
    let affine = affine?;
    let tau = recognise::tau(&affine)?;
    let fill = shape
        .fill
        .as_ref()
        .filter(|fill| look(&fill.paint, paint.fill).visible);
    let outline = match &shape.stroke {
        Some(stroke) => look(&stroke.paint, paint.stroke.unwrap_or(paint.fill))
            .visible
            .then_some(PartOf::OwnStroke),
        None => paint
            .stroke
            .map(of_color)
            .filter(|look| look.visible)
            .map(|_| PartOf::Inherited(f64::from(paint.stroke_width))),
    };
    if fill.is_none() && outline.is_none() {
        return None;
    }
    // A polygon fails recognition anyway; this finds out in one pass with no allocation. A stroke
    // may run in any direction, so only the fill is asked. A path long enough to be recognised on
    // several threads is left to them: there, the pass would cost a read of every element.
    if fill.is_some() && elements < recognise::PARALLEL_ELEMENTS {
        let (path, scale) = match super::recognised::uniform(source.fit) {
            Some((scale, _)) => (&shape.path, scale),
            None => (&source.placed().path, 1.0),
        };
        if !straight_on_axes(path, tau / scale) {
            return None;
        }
    }
    let declined = || {
        if elements >= REMEMBERED {
            masks.marks_declined(id);
        }
        None
    };
    let recognise_part =
        |part| recognised_in_source(source, Outline::Path, part, tau, MAX_MARK_PRIMS, masks);
    let filled = match fill {
        Some(fill) => match recognise_part(PartOf::Fill(fill.rule)) {
            Some(found) => Some(found),
            None => return declined(),
        },
        None => None,
    };
    let stroked = match outline {
        Some(part) => match recognise_part(part) {
            Some(found) => Some(found),
            None => return declined(),
        },
        None => None,
    };
    // A part with no prims encloses no area and draws nothing.
    let filled = filled.filter(|(found, _)| found.count > 0);
    let stroked = stroked.filter(|(found, _)| found.count > 0);
    if filled.is_none() && stroked.is_none() {
        return None;
    }
    let fills = match (filled, fill) {
        (Some((found, place)), Some(fill)) => {
            Some(lower(found, place, &affine, Some(fill.rule), masks)?)
        }
        _ => None,
    };
    let strokes = match stroked {
        Some((found, place)) => Some(lower(found, place, &affine, None, masks)?),
        None => None,
    };

    let links = if shape.clips.is_empty() {
        SmallVec::new()
    } else {
        let inks: SmallVec<[Rect<DevicePx, Device>; 2]> = [&fills, &strokes]
            .into_iter()
            .flatten()
            .map(|(lowered, _)| rect(lowered.ink))
            .collect();
        clip_links(
            source,
            &affine,
            tau,
            masks,
            placement.transform,
            &inks,
            MARGIN / smallest_scale(&affine),
        )?
    };

    // Everything is decided. From here on, the shape is drawn.
    let mut clip = placement.clip;
    for link in links {
        clip = scene.clips.push(clip, link);
        scene.note_minted_clip(clip);
    }
    let mut pushed = 0;
    if let (Some((lowered, place)), Some(fill)) = (fills, fill) {
        let paint = reference(scene, &placed_paint(source, &fill.paint), paint.fill);
        pushed += push(scene, lowered, place, paint, clip, placement);
    }
    if let Some((lowered, place)) = strokes {
        let paint = stroke_paint(scene, source, paint);
        pushed += push(scene, lowered, place, paint, clip, placement);
    }
    Some(pushed)
}

/// Pushes one part as one mark whose payload `place` maps to the fragment's space, and returns
/// how many survived.
fn push(
    scene: &mut Scene,
    lowered: Lowered,
    place: kurbo::Affine,
    paint: PaintRef,
    clip: zgui_scene::ClipId,
    placement: VectorPlacement,
) -> usize {
    let mut item = MarkItem::new(rect(lowered.ink), paint, lowered.payload.counts());
    let [a, b, c, d, e, f] = place.as_coeffs();
    item.flags = lowered.flags;
    item.clip = clip.0;
    item.transform = placement.transform.index();
    item.half_width = lowered.half_width;
    item.axes = [a as f32, b as f32, c as f32, d as f32];
    item.origin = [e as f32, f as f32];
    usize::from(scene.push_marks(item, lowered.payload).is_some())
}

/// An `[x0, y0, x1, y1]` rectangle in the shape's space.
fn rect([x0, y0, x1, y1]: [f32; 4]) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x0), DevicePx(y0)),
        Size::new(DevicePx(x1 - x0), DevicePx(y1 - y0)),
    )
}

/// The shortest length one local unit becomes on the device: the smallest singular value.
fn smallest_scale(affine: &zgui_geom::Affine2) -> f32 {
    let [a, b, c, d] = [affine.a, affine.b, affine.c, affine.d].map(f64::from);
    let p = a * a + b * b;
    let q = c * c + d * d;
    let r = a * c + b * d;
    let spread = (((p - q) / 2.0).powi(2) + r * r).sqrt();
    ((p + q) / 2.0 - spread).max(0.0).sqrt().max(1.0e-6) as f32
}

/// The payload of one part, its flags and its ink, with the matrix that maps the payload to the
/// fragment's space, or `None` for polyline caps no item can draw, or for a fill whose prims
/// overlap and are not one union.
///
/// `found` is in the units it was recognised in, `place` maps it to the fragment's space and
/// `affine` maps that to the device.
///
/// The interior of overlapping subpaths is their union under the nonzero rule when they all turn
/// one way. Turning both ways, or under the even-odd rule, an inner subpath is a hole, which no
/// sum of coverage draws. `rule` is the fill rule of a fill, and `None` for a stroke, whose outline
/// is always the union of its segments'.
fn lower(
    found: Arc<Decomposition>,
    place: kurbo::Affine,
    affine: &zgui_geom::Affine2,
    rule: Option<peniko::Fill>,
    masks: &dyn VectorMaskSource,
) -> Option<(Lowered, kurbo::Affine)> {
    let device = compose(affine, place);
    let union = found.count > 1 && !apart(&found, &device);
    let holes = match rule {
        Some(peniko::Fill::EvenOdd) => true,
        Some(peniko::Fill::NonZero) => found.orientation == Orientation::Mixed,
        None => false,
    };
    if union && holes {
        return None;
    }
    let local = place.transform_rect_bbox(kurbo::Rect::new(
        f64::from(found.ink[0]),
        f64::from(found.ink[1]),
        f64::from(found.ink[2]),
        f64::from(found.ink[3]),
    ));
    let ink = [local.x0, local.y0, local.x1, local.y1].map(|value| value as f32);
    let half_width = found.half_width;
    // A recognition the cache holds is lowered once, and its payload is the same allocation on
    // every encode.
    let cached = Arc::strong_count(&found) > 1;
    let held = cached
        .then(|| masks.payloads()?.shape(&found, union))
        .flatten();
    if let Some((payload, flags)) = held {
        let lowered = Lowered {
            payload,
            flags,
            ink,
            half_width,
        };
        return Some((lowered, place));
    }
    let (payload, flags) = if cached {
        let (payload, flags) = payload_of(Arc::clone(&found), union)?;
        let payload = Arc::new(payload);
        if let Some(mut payloads) = masks.payloads() {
            payloads.insert_shape(&found, union, Arc::clone(&payload), flags);
        }
        (payload, flags)
    } else {
        let (payload, flags) = payload_of(found, union)?;
        (Arc::new(payload), flags)
    };
    let lowered = Lowered {
        payload,
        flags,
        ink,
        half_width,
    };
    Some((lowered, place))
}

/// `affine` after `place`: the map from the payload to the device.
fn compose(affine: &zgui_geom::Affine2, place: kurbo::Affine) -> zgui_geom::Affine2 {
    let [a, b, c, d, e, f] = place.as_coeffs().map(|value| value as f32);
    zgui_geom::Affine2::new(a, b, c, d, e, f).then(*affine)
}

/// The payload of `found` and its flags, or `None` for polyline caps no item can draw.
fn payload_of(found: Arc<Decomposition>, union: bool) -> Option<(MarkPayload, u32)> {
    let mut vertices = Vec::new();
    let mut flags = runs(&found, &mut vertices)?;
    // A result no cache holds is moved into the payload rather than copied.
    let found = Arc::unwrap_or_clone(found);
    let mut payload = MarkPayload {
        discs: found.discs,
        boxes: found
            .boxes
            .iter()
            .map(|prim| MarkBox {
                rect: prim.rect,
                radii: prim.radii,
                shape: [prim.exponent, prim.border, 0.0, 0.0],
            })
            .collect(),
        vertices,
    };
    if union {
        flags |= MarkFlags::UNION;
        dedupe(&mut payload.discs, found.ink);
    }
    Some((payload, flags))
}

/// Writes the capsules of `found` as polyline runs into `vertices`, and returns the cap flags.
///
/// A capsule continues the run before it when it starts where the run ends and both ends that
/// meet there are round: the round join. Any other capsule starts a run. Every run has to start
/// with one cap and end with one cap, which become the item's; `None` otherwise.
fn runs(found: &Decomposition, vertices: &mut Vec<[f32; 2]>) -> Option<u32> {
    if found.capsules.is_empty() {
        return Some(0);
    }
    let separator = [f32::NAN, f32::NAN];
    vertices.reserve(found.capsules.len() * 2 + 2);
    vertices.push(separator);
    let mut start_cap: Option<u8> = None;
    let mut end_cap: Option<u8> = None;
    let mut last: Option<([f32; 2], u8)> = None;
    for (capsule, &caps) in found.capsules.iter().zip(&found.caps) {
        let [x0, y0, x1, y1] = *capsule;
        let (start, end) = (caps & 3, caps >> 2);
        let continues = last.is_some_and(|(point, cap)| {
            point == [x0, y0] && cap == recognise::ROUND && start == recognise::ROUND
        });
        if continues {
            vertices.push([x1, y1]);
        } else {
            if let Some((_, cap)) = last {
                if *end_cap.get_or_insert(cap) != cap {
                    return None;
                }
                vertices.push(separator);
            }
            if *start_cap.get_or_insert(start) != start {
                return None;
            }
            vertices.push([x0, y0]);
            vertices.push([x1, y1]);
        }
        last = Some(([x1, y1], end));
    }
    let (_, cap) = last?;
    if *end_cap.get_or_insert(cap) != cap {
        return None;
    }
    vertices.push(separator);
    Some(MarkFlags::caps(
        u32::from(start_cap.unwrap_or(0)),
        u32::from(end_cap.unwrap_or(0)),
    ))
}

/// Whether the prims of `found` are at least two device pixels apart.
///
/// Measured one prim at a time, so prims that overlap answer after the first overlap.
fn apart(found: &Decomposition, affine: &zgui_geom::Affine2) -> bool {
    let half = found.half_width;
    let device = |bounds: [f32; 4]| {
        let on = affine.transform_rect(rect(bounds));
        [on.left().0, on.top().0, on.right().0, on.bottom().0]
    };
    let (discs, boxes) = (found.discs.len(), found.boxes.len());
    let local = |index: usize| {
        if index < discs {
            let [cx, cy, outer, _] = found.discs[index];
            [cx - outer, cy - outer, cx + outer, cy + outer]
        } else if index < discs + boxes {
            found.boxes[index - discs].rect
        } else {
            // Every end reaches at most half the width past its point, whatever its cap.
            let [x0, y0, x1, y1] = found.capsules[index - discs - boxes];
            [
                x0.min(x1) - half,
                y0.min(y1) - half,
                x0.max(x1) + half,
                y0.max(y1) + half,
            ]
        }
    };
    // A side of a prim's bounds is at most its longest extent and a stroke width; a matrix
    // stretches a side by at most the sum of the magnitudes of its row.
    let stretch = (affine.a.abs() + affine.c.abs()).max(affine.b.abs() + affine.d.abs());
    let largest = (found.max_extent + 2.0 * half) * stretch;
    recognise::separated_with(found.count, largest, MARGIN, |index| device(local(index)))
}

/// Drops discs identical to one already held, once the discs cover their ink many times over.
///
/// Under a union an identical disc adds nothing, so dropping it changes no pixel.
fn dedupe(discs: &mut Vec<[f32; 4]>, ink: [f32; 4]) {
    let area = (ink[2] - ink[0]) * (ink[3] - ink[1]);
    let covered: f32 = discs
        .iter()
        .map(|disc| core::f32::consts::PI * disc[2] * disc[2])
        .sum();
    if area.is_nan() || area <= 0.0 || covered / area <= OVERDRAW {
        return;
    }
    let mut seen: FxHashSet<(i64, i64, u32, u32)> = FxHashSet::default();
    discs.retain(|&[cx, cy, outer, inner]| {
        seen.insert((
            (f64::from(cx) * 64.0).round() as i64,
            (f64::from(cy) * 64.0).round() as i64,
            outer.to_bits(),
            inner.to_bits(),
        ))
    });
}

#[cfg(test)]
mod tests;
