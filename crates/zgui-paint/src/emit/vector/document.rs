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

use zgui_color::{Color, ColorSpace, HueInterpolation};
use zgui_geom::{Device, DevicePx, Rect};
use zgui_scene::kurbo;
use zgui_scene::{
    GradientKind, Paint, PaintRef, Scene, VectorClip, VectorId, VectorItem, VectorStroke,
};

use crate::emit::vector::{ShapePaint, VectorPlacement, under};

/// Emits one shape, and returns how many primitives were pushed.
pub fn emit(
    scene: &mut Scene,
    id: VectorId,
    shape: &zgui_svg::Shape,
    paint: &ShapePaint,
    placement: VectorPlacement,
) -> usize {
    let clips: Vec<VectorClip> = shape
        .clips
        .iter()
        .map(|clip| VectorClip {
            path: Arc::clone(&clip.path),
            rule: clip.rule,
        })
        .collect();
    let stroke = stroke_of(scene, shape, paint);
    let local = ink_of(shape, stroke.as_ref());
    let ink = under(scene, placement.transform, local);

    let mut pushed = 0;
    if let Some(fill) = &shape.fill {
        let reference = reference(scene, &fill.paint, paint.fill);
        let mut item = VectorItem::filled(id, Arc::clone(&shape.path), reference)
            .clipped(placement.clip)
            .inside(clips.clone());
        item.fill_rule = fill.rule;
        item.ink = ink;
        item.local_ink = local;
        item.transform = Some(placement.transform);
        pushed += usize::from(scene.push_vector(item).is_some());
    }
    if let Some(stroke) = stroke {
        let mut item = VectorItem::styled(id, Arc::clone(&shape.path), stroke)
            .clipped(placement.clip)
            .inside(clips);
        item.ink = ink;
        item.local_ink = local;
        item.transform = Some(placement.transform);
        pushed += usize::from(scene.push_vector(item).is_some());
    }
    pushed
}

/// What strokes one shape, which is the shape's own stroke or the element's.
///
/// A shape that named no stroke is stroked only when the element asked for one through
/// `--zgui-stroke`. That is what makes a bare outline strokeable from a stylesheet without giving
/// every shape of a vector document a stroke it never asked for.
fn stroke_of(
    scene: &mut Scene,
    shape: &zgui_svg::Shape,
    paint: &ShapePaint,
) -> Option<VectorStroke> {
    match &shape.stroke {
        Some(stroke) => Some(VectorStroke {
            paint: reference(scene, &stroke.paint, paint.stroke.unwrap_or(paint.fill)),
            style: stroke.style.clone(),
        }),
        None => {
            let color = paint.stroke?;
            Some(VectorStroke::solid(
                PaintRef::solid(scene.paints.solid(color)),
                paint.stroke_width,
            ))
        }
    }
}

/// The rectangle one shape can put ink in, in its own space.
fn ink_of(shape: &zgui_svg::Shape, stroke: Option<&VectorStroke>) -> Rect<DevicePx, Device> {
    match stroke {
        Some(stroke) => {
            VectorItem::styled(VectorId(0), Arc::clone(&shape.path), stroke.clone()).ink
        }
        None => VectorItem::filled(VectorId(0), Arc::clone(&shape.path), PaintRef::NONE).ink,
    }
}

/// The interned paint one of a document's paints becomes, given the inherited colour.
fn reference(scene: &mut Scene, paint: &zgui_svg::Paint, inherited: Color) -> PaintRef {
    match paint {
        zgui_svg::Paint::Solid(ink) => PaintRef::solid(scene.paints.solid(ink.resolve(inherited))),
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
            scene.paints.add(Paint::Gradient {
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
fn point(point: kurbo::Point) -> zgui_geom::Point<DevicePx, Device> {
    zgui_geom::Point::new(DevicePx(point.x as f32), DevicePx(point.y as f32))
}
