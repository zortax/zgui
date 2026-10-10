//! Recognised shapes drawn as marks: the route for many prims, and for prims that overlap.
//!
//! Each part of a shape whose subpaths are all circles, axis-aligned ellipses, rectangles, rounded
//! rectangles or simple strokes becomes one [`MarkItem`] over a shared payload of its prims. Prims
//! two device pixels apart are painted one by one. Prims that come closer are summed into one
//! coverage first and painted through the sum once, which is the union a path paints.

use std::sync::Arc;

use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_scene::{MarkBox, MarkFlags, MarkItem, MarkPayload, PaintRef, Scene, VectorId};

use super::analytic::{clip_links, look, of_color};
use super::document::{reference, stroke_paint};
use super::recognise::{self, Decomposition};
use super::recognised::{Outline, PartOf, placed_paint, recognised, straight_on_axes};
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
    /// The prims.
    payload: MarkPayload,
    /// [`MarkFlags`] bits.
    flags: u32,
    /// What the prims paint together, in the shape's local space.
    ink: [f32; 4],
    /// Half the polyline stroke width.
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
    let affine = scene
        .spatial
        .resolve(placement.transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2)?;
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
    // may run in any direction, so only the fill is asked.
    if fill.is_some() {
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
    let recognise_part = |part| recognised(source, Outline::Path, part, tau, MAX_MARK_PRIMS, masks);
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
    let filled = filled.filter(|found| found.count > 0);
    let stroked = stroked.filter(|found| found.count > 0);
    if filled.is_none() && stroked.is_none() {
        return None;
    }
    let fills = match &filled {
        Some(found) => Some(lower(found, &affine)?),
        None => None,
    };
    let strokes = match &stroked {
        Some(found) => Some(lower(found, &affine)?),
        None => None,
    };

    let margin = MARGIN / smallest_scale(&affine);
    let inks: SmallVec<[Rect<DevicePx, Device>; 2]> = [&fills, &strokes]
        .into_iter()
        .flatten()
        .map(|lowered| rect(lowered.ink))
        .collect();
    let links = clip_links(
        source,
        &affine,
        tau,
        masks,
        placement.transform,
        &inks,
        margin,
    )?;

    // Everything is decided. From here on, the shape is drawn.
    let mut clip = placement.clip;
    for link in links {
        clip = scene.clips.push(clip, link);
        scene.note_minted_clip(clip);
    }
    let mut pushed = 0;
    if let (Some(lowered), Some(fill)) = (fills, fill) {
        let paint = reference(scene, &placed_paint(source, &fill.paint), paint.fill);
        pushed += push(scene, lowered, paint, clip, placement);
    }
    if let Some(lowered) = strokes {
        let paint = stroke_paint(scene, source, paint);
        pushed += push(scene, lowered, paint, clip, placement);
    }
    Some(pushed)
}

/// Pushes one part as one mark, and returns how many survived.
fn push(
    scene: &mut Scene,
    lowered: Lowered,
    paint: PaintRef,
    clip: zgui_scene::ClipId,
    placement: VectorPlacement,
) -> usize {
    let mut item = MarkItem::new(rect(lowered.ink), paint, lowered.payload.counts());
    item.flags = lowered.flags;
    item.clip = clip.0;
    item.transform = placement.transform.index();
    item.half_width = lowered.half_width;
    usize::from(scene.push_marks(item, Arc::new(lowered.payload)).is_some())
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

/// The payload of one part, its flags and its ink, or `None` for polyline caps no item can draw.
fn lower(found: &Decomposition, affine: &zgui_geom::Affine2) -> Option<Lowered> {
    let mut payload = MarkPayload {
        discs: found.discs.clone(),
        boxes: found
            .boxes
            .iter()
            .map(|prim| MarkBox {
                rect: prim.rect,
                radii: prim.radii,
                shape: [prim.exponent, prim.border, 0.0, 0.0],
            })
            .collect(),
        vertices: Vec::new(),
    };
    let caps = runs(found, &mut payload.vertices)?;
    let mut flags = caps;
    if found.count > 1 && !apart(found, affine) {
        flags |= MarkFlags::UNION;
        dedupe(&mut payload.discs, found.ink);
    }
    Some(Lowered {
        payload,
        flags,
        ink: found.ink,
        half_width: found.half_width,
    })
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
fn apart(found: &Decomposition, affine: &zgui_geom::Affine2) -> bool {
    let half = found.half_width;
    let device = |bounds: [f32; 4]| {
        let on = affine.transform_rect(rect(bounds));
        [on.left().0, on.top().0, on.right().0, on.bottom().0]
    };
    let mut rects: Vec<[f32; 4]> = Vec::with_capacity(found.count);
    rects.extend(
        found
            .discs
            .iter()
            .map(|&[cx, cy, outer, _]| device([cx - outer, cy - outer, cx + outer, cy + outer])),
    );
    rects.extend(found.boxes.iter().map(|prim| device(prim.rect)));
    rects.extend(found.capsules.iter().map(|&[x0, y0, x1, y1]| {
        // Every end reaches at most half the width past its point, whatever its cap.
        device([
            x0.min(x1) - half,
            y0.min(y1) - half,
            x0.max(x1) + half,
            y0.max(y1) + half,
        ])
    }));
    recognise::separated(&rects, MARGIN)
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
