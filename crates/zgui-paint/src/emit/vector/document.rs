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
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_scene::kurbo;
use zgui_scene::{
    GradientKind, MonoSprite, Paint, PaintRef, Scene, VectorClip, VectorId, VectorItem,
    VectorStroke,
};

use crate::content::vectors::{VectorMaskRequest, VectorMaskSource, VectorMaskStyle};
use crate::emit::vector::{ShapePaint, VectorPlacement, under};

/// Emits one shape, and returns how many primitives were pushed.
pub fn emit(
    scene: &mut Scene,
    id: VectorId,
    shape: &zgui_svg::Shape,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> usize {
    if let Some(pushed) = emit_mask(scene, shape, paint, masks, placement) {
        return pushed;
    }
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

/// Emits a small solid translation-only shape as an atlas mask, or declines the fast path.
fn emit_mask(
    scene: &mut Scene,
    shape: &zgui_svg::Shape,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> Option<usize> {
    if !shape.clips.is_empty() {
        return None;
    }
    let fill = shape.fill.as_ref().and_then(|fill| match &fill.paint {
        zgui_svg::Paint::Solid(ink) => {
            let color = ink.resolve(paint.fill);
            (color.alpha() != 0.0).then_some((color, fill.rule))
        }
        zgui_svg::Paint::Gradient(_) => None,
    });
    let inherited_stroke = (shape.stroke.is_none() && paint.stroke.is_some())
        .then(|| kurbo::Stroke::new(f64::from(paint.stroke_width)));
    let stroke = match &shape.stroke {
        Some(stroke) => match &stroke.paint {
            zgui_svg::Paint::Solid(ink) => {
                let color = ink.resolve(paint.stroke.unwrap_or(paint.fill));
                (color.alpha() != 0.0).then_some((color, &stroke.style))
            }
            zgui_svg::Paint::Gradient(_) => None,
        },
        None => paint
            .stroke
            .filter(|color| color.alpha() != 0.0)
            .zip(inherited_stroke.as_ref())
            .map(|(color, stroke)| (color, stroke)),
    };
    // One mask has one tint and one coverage operation. A fill plus a stroke may use different
    // paints and their overlapping antialiasing does not equal either operation alone, so it stays
    // on the general rasteriser. Stroke-only icons are the important second common case.
    let (color, style, stroke_for_ink) = match (fill, stroke) {
        (Some((color, rule)), None) => (color, VectorMaskStyle::Fill(rule), None),
        (None, Some((color, stroke))) => (
            color,
            VectorMaskStyle::Stroke(stroke),
            Some(VectorStroke {
                paint: PaintRef::NONE,
                style: stroke.clone(),
            }),
        ),
        _ => return None,
    };
    let affine = scene
        .spatial
        .resolve(placement.transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2)?;
    const EPSILON: f32 = 1.0e-6;
    if (affine.a - 1.0).abs() > EPSILON
        || affine.b.abs() > EPSILON
        || affine.c.abs() > EPSILON
        || (affine.d - 1.0).abs() > EPSILON
    {
        return None;
    }

    let local = ink_of(shape, stroke_for_ink.as_ref());
    let left = local.left().0.floor();
    let top = local.top().0.floor();
    let right = local.right().0.ceil();
    let bottom = local.bottom().0.ceil();
    if ![left, top, right, bottom]
        .iter()
        .all(|edge| edge.is_finite())
    {
        return None;
    }
    let width = (right - left) as i32;
    let height = (bottom - top) as i32;
    if width <= 0 || height <= 0 || width > 96 || height > 96 {
        return None;
    }
    let bounds = Rect::new(
        Point::new(left as i32, top as i32),
        Size::new(width, height),
    );
    let mask = masks.vector_mask(VectorMaskRequest {
        path: &shape.path,
        style,
        scale: placement.scale,
        bounds,
    })?;
    let sprite_bounds = Rect::new(
        Point::new(DevicePx(left), DevicePx(top)),
        Size::new(DevicePx(width as f32), DevicePx(height as f32)),
    );
    let mut sprite = MonoSprite::new(sprite_bounds, mask.tile, color).clipped(placement.clip);
    sprite.transform = placement.transform.index();
    Some(usize::from(scene.push_mono_sprite(sprite).is_some()))
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
