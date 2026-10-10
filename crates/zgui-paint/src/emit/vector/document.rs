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
use zgui_profile::{Counter, counter};
use zgui_scene::kurbo;
use zgui_scene::{
    GradientKind, MonoSprite, Paint, PaintRef, Scene, VectorClip, VectorId, VectorItem,
    VectorStroke,
};

use crate::content::vectors::{VectorMaskRequest, VectorMaskSource, VectorMaskStyle};
use crate::emit::vector::{ShapePaint, ShapeSource, VectorPlacement, under};

use super::analytic::emit_analytic;
use super::marks::emit_marks;
use super::{ShapeEmission, VectorRoute};

/// Emits one shape, and returns how many primitives were pushed.
pub fn emit(
    scene: &mut Scene,
    id: VectorId,
    shape: &zgui_svg::Shape,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> usize {
    emit_tracked(
        scene,
        id,
        &ShapeSource::placed_shape(shape),
        paint,
        masks,
        placement,
    )
    .pushed
}

/// Emits one shape and reports which raster path it selected.
///
/// Every route reads the source shape in its own space. The mask route maps it through the fit as
/// it rasterises, and the general route hands the source path to the rasteriser with the fit as
/// the item's placement, so a new fit makes no new path.
pub(crate) fn emit_tracked(
    scene: &mut Scene,
    id: VectorId,
    source: &ShapeSource<'_>,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> ShapeEmission {
    // Resolved once for both recognising routes, which read the same matrix.
    let affine = scene
        .spatial
        .resolve(placement.transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2);
    if let Some(pushed) = emit_analytic(scene, id, source, paint, masks, placement, affine, false) {
        counter::bump(Counter::VectorRouteAnalytic);
        return ShapeEmission {
            pushed,
            route: Some(VectorRoute::Analytic),
        };
    }
    if let Some(pushed) = emit_marks(scene, id, source, paint, masks, placement, affine, false) {
        counter::bump(Counter::VectorRouteMarks);
        return ShapeEmission {
            pushed,
            route: Some(VectorRoute::Marks),
        };
    }
    // A fit with no area is a box of no size, and nothing drawn into it reaches a pixel.
    let area = source.fit.determinant();
    if !area.is_finite() || area.abs() <= 1.0e-12 {
        return ShapeEmission::default();
    }
    if let Some(pushed) = emit_mask(scene, id, source, paint, masks, placement) {
        counter::bump(Counter::VectorRouteMask);
        return ShapeEmission {
            pushed,
            route: Some(VectorRoute::AtlasMask),
        };
    }
    // A transform with no area behind it — `scale(0)`, and a projection the display list flattened
    // to one — takes every outline to a line or a point, so nothing it draws can reach a pixel. It
    // is refused here rather than rasterised: a shape hidden this way would otherwise be the item
    // that builds the general rasteriser, for a picture with no extent.
    if flattened(scene, placement) {
        return ShapeEmission::default();
    }
    // The source path, clips and paints, in the shape's own space: the fit is the items'
    // placement, so the rasteriser's encoding of the path outlives a new fit.
    let shape = source.shape;
    let clips: Vec<VectorClip> = shape
        .clips
        .iter()
        .map(|clip| VectorClip {
            path: Arc::clone(&clip.path),
            rule: clip.rule,
        })
        .collect();
    let stroke = stroke_of(scene, shape, paint, source.fit);
    let has_stroke = stroke.is_some();
    // Both items take the ink of the stroked shape, so the fill and the stroke order as one.
    let reach = stroke
        .as_ref()
        .map_or(0.0, |stroke| f64::from(stroke.reach()));
    let local = ink_of_bounds(
        source
            .fit
            .transform_rect_bbox(shape.path.control_box().inflate(reach, reach)),
        0.0,
    );
    let ink = under(scene, placement.transform, local);

    let mut pushed = 0;
    if let Some(fill) = &shape.fill {
        let reference = reference(scene, &fill.paint, paint.fill);
        let mut item = VectorItem::filled(id, Arc::clone(&shape.path), reference)
            .placed(source.fit)
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
            .placed(source.fit)
            .clipped(placement.clip)
            .inside(clips);
        item.ink = ink;
        item.local_ink = local;
        item.transform = Some(placement.transform);
        pushed += usize::from(scene.push_vector(item).is_some());
    }
    if shape.fill.is_some() || has_stroke {
        counter::bump(Counter::VectorRouteGeneral);
    }
    ShapeEmission {
        pushed,
        route: (shape.fill.is_some() || has_stroke).then_some(VectorRoute::GeneralRaster),
    }
}

/// The most coverage texels one mask may occupy.
///
/// A mask holds one byte per texel, so this is 64 KiB and several fit one default atlas texture.
const MAX_MASK_TEXELS: f32 = 64.0 * 1024.0;

/// The widest a mask may be, which is one default atlas page.
///
/// Past this the atlas grows a texture for the tile alone, and a shape big enough to need one is
/// big enough for the general vector path.
const MAX_MASK_WIDTH: f32 = 1024.0;

/// The tallest a mask may be.
///
/// A shelf allocator is asymmetric and so is this. A wide, thin mask — an axis, a rule, a divider —
/// opens a shelf as thin as itself and costs the page almost nothing. A tall one opens a shelf as
/// tall as itself that shorter tiles will not be placed in, so a single sliver can spend a quarter
/// of a page shared with every glyph on the screen.
const MAX_MASK_HEIGHT: f32 = 256.0;

/// Whether a mask of this size is worth putting in a page shared with the glyphs.
fn fits(width: f32, height: f32) -> bool {
    width >= 1.0
        && height >= 1.0
        && width <= MAX_MASK_WIDTH
        && height <= MAX_MASK_HEIGHT
        && width * height <= MAX_MASK_TEXELS
}

/// How much residual rotation a linear part may carry and still be read as axis-preserving.
///
/// Relative, because the residue a matrix carries is proportional to the lengths in it: an absolute
/// bound refuses a quarter turn composed through three nested transforms as readily as it refuses a
/// real one.
const EPSILON: f32 = 1.0e-4;

/// Whether a placement's transform takes every outline under it to something with no area.
fn flattened(scene: &Scene, placement: VectorPlacement) -> bool {
    let Some(affine) = scene
        .spatial
        .resolve(placement.transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2)
    else {
        return false;
    };
    let area = affine.a * affine.d - affine.b * affine.c;
    !area.is_finite() || area.abs() <= f32::EPSILON
}

/// Mask texels per unit of each of a shape's own axes, or `None` to decline the fast path.
///
/// The monochrome pages are sampled without filtering, so a mask is exact only where the shape's
/// own axes land on the device's. That is every scale, every mirror and every quarter turn — which
/// is what interfaces are built from — and it is not an arbitrary rotation or a shear, which stay
/// on the general rasteriser.
///
/// A pure rotation gives a density of one on both axes, so a shape that turns rasterises once and
/// shares that tile at every angle.
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
    // A stroke is measured along the outline, so it has one width whatever direction the outline
    // runs in. A map that scales the two axes differently gives it two, and no single mask says
    // what that shape looks like.
    if stroked && (kx - ky).abs() > EPSILON * kx.max(ky) {
        return None;
    }
    Some([kx, ky])
}

/// Emits a small solid shape as an atlas mask, or declines the fast path.
///
/// The mask is rasterised from the source path through the fit, so no placed copy of the path is
/// made.
fn emit_mask(
    scene: &mut Scene,
    id: VectorId,
    source: &ShapeSource<'_>,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> Option<usize> {
    let shape = source.shape;
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
    // The mask is measured in the fragment's space: the shape's own stroke is scaled by the fit,
    // and an inherited stroke is in device pixels already.
    let own_stroke = shape.stroke.as_ref().map(|stroke| {
        zgui_svg::document::place::scaled(
            &stroke.style,
            zgui_svg::document::place::uniform_scale(source.fit),
        )
    });
    let inherited_stroke = (shape.stroke.is_none() && paint.stroke.is_some())
        .then(|| kurbo::Stroke::new(f64::from(paint.stroke_width)));
    let stroke = match &shape.stroke {
        Some(stroke) => match &stroke.paint {
            zgui_svg::Paint::Solid(ink) => {
                let color = ink.resolve(paint.stroke.unwrap_or(paint.fill));
                (color.alpha() != 0.0)
                    .then_some(color)
                    .zip(own_stroke.as_ref())
            }
            zgui_svg::Paint::Gradient(_) => None,
        },
        None => paint
            .stroke
            .filter(|color| color.alpha() != 0.0)
            .zip(inherited_stroke.as_ref()),
    };
    if fill.is_none() && stroke.is_none() {
        return None;
    }
    // The general route strokes with the pen the fit stretches, and one mask width cannot say
    // that. The same rule `density_of` applies to the transform.
    if stroke.is_some() && !uniform_axes(source.fit) {
        return None;
    }
    let affine = scene
        .spatial
        .resolve(placement.transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2)?;
    let density = density_of(&affine, stroke.is_some())?;
    let outline = stroke.map(|(_, style)| VectorStroke {
        paint: PaintRef::NONE,
        style: style.clone(),
    });

    // A shape with both a fill and a stroke becomes two sprites, which is what the general
    // rasteriser makes of it as well: two items, the stroke composited over the fill. Each is
    // measured against its own ink, because a stroke puts ink outside the interior it follows.
    //
    // Both parts qualify or neither does. Half a shape from a sprite and half from a vector pass
    // has no order between the halves, and one part alone draws a different picture.
    let bounds = source.fit.transform_rect_bbox(shape.path.control_box());
    let filled = match fill {
        Some((color, rule)) => Some(mask_sprite(
            id,
            source,
            masks,
            placement,
            density,
            color,
            VectorMaskStyle::Fill(rule),
            ink_of_bounds(bounds, 0.0),
        )?),
        None => None,
    };
    let stroked = match stroke {
        Some((color, style)) => Some(mask_sprite(
            id,
            source,
            masks,
            placement,
            density,
            color,
            VectorMaskStyle::Stroke(style),
            ink_of_bounds(bounds, outline.as_ref().map_or(0.0, VectorStroke::reach)),
        )?),
        None => None,
    };

    let mut pushed = 0;
    for sprite in [filled, stroked].into_iter().flatten() {
        pushed += usize::from(scene.push_mono_sprite(sprite).is_some());
    }
    Some(pushed)
}

/// One part of a shape as a tinted sprite over a coverage tile, or `None` to decline the fast path.
///
/// `local` is the part's ink in the fragment's space.
#[allow(clippy::too_many_arguments)]
fn mask_sprite(
    owner: VectorId,
    source: &ShapeSource<'_>,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
    density: [f32; 2],
    color: Color,
    style: VectorMaskStyle<'_>,
    local: Rect<DevicePx, Device>,
) -> Option<MonoSprite> {
    let [kx, ky] = density;
    // Measured in mask space, which is the shape's own space scaled by the density. The sprite is
    // handed back the same rectangle divided out again, so it keeps riding `placement.transform`
    // and every clip, draw order and replay offset is stated where it always was.
    let left = (local.left().0 * kx).floor();
    let top = (local.top().0 * ky).floor();
    let right = (local.right().0 * kx).ceil();
    let bottom = (local.bottom().0 * ky).ceil();
    if ![left, top, right, bottom]
        .iter()
        .all(|edge| edge.is_finite())
    {
        return None;
    }
    // Tested before the cast, which saturates rather than wraps: a saturated edge would pass a
    // budget stated in the integers it saturated to.
    if !fits(right - left, bottom - top) {
        return None;
    }
    let width = (right - left) as i32;
    let height = (bottom - top) as i32;
    let bounds = Rect::new(
        Point::new(left as i32, top as i32),
        Size::new(width, height),
    );
    let mask = masks.vector_mask(VectorMaskRequest {
        owner,
        path: &source.shape.path,
        placement: source.fit,
        style,
        density,
        scale: placement.scale,
        bounds,
    })?;
    let sprite_bounds = Rect::new(
        Point::new(DevicePx(left / kx), DevicePx(top / ky)),
        Size::new(DevicePx(width as f32 / kx), DevicePx(height as f32 / ky)),
    );
    let mut sprite = MonoSprite::new(sprite_bounds, mask.tile, color).clipped(placement.clip);
    sprite.transform = placement.transform.index();
    Some(sprite)
}

/// What strokes one shape in its own space, which is the shape's own stroke or the element's.
///
/// A shape that named no stroke is stroked only when the element asked for one through
/// `--zgui-stroke`. That is what makes a bare outline strokeable from a stylesheet without giving
/// every shape of a vector document a stroke it never asked for. The element's width is in device
/// pixels, so it is divided by the scale `fit` applies; a fit with no scale strokes nothing.
pub(super) fn stroke_of(
    scene: &mut Scene,
    shape: &zgui_svg::Shape,
    paint: &ShapePaint,
    fit: kurbo::Affine,
) -> Option<VectorStroke> {
    match &shape.stroke {
        Some(stroke) => Some(VectorStroke {
            paint: reference(scene, &stroke.paint, paint.stroke.unwrap_or(paint.fill)),
            style: stroke.style.clone(),
        }),
        None => {
            let color = paint.stroke?;
            let width =
                f64::from(paint.stroke_width) / zgui_svg::document::place::uniform_scale(fit);
            if !width.is_finite() {
                return None;
            }
            Some(VectorStroke {
                paint: PaintRef::solid(scene.paints.solid(color)),
                style: kurbo::Stroke::new(width),
            })
        }
    }
}

/// Whether `fit` scales both of its axes alike, within [`EPSILON`] of the larger.
fn uniform_axes(fit: kurbo::Affine) -> bool {
    let [a, b, c, d, _, _] = fit.as_coeffs();
    let (kx, ky) = (a.hypot(b), c.hypot(d));
    (kx - ky).abs() <= f64::from(EPSILON) * kx.max(ky)
}

/// `bounds` grown by `reach` on every side.
fn ink_of_bounds(bounds: kurbo::Rect, reach: f32) -> Rect<DevicePx, Device> {
    let reach = f64::from(reach);
    Rect::new(
        Point::new(
            DevicePx((bounds.x0 - reach) as f32),
            DevicePx((bounds.y0 - reach) as f32),
        ),
        Size::new(
            DevicePx((bounds.width() + 2.0 * reach) as f32),
            DevicePx((bounds.height() + 2.0 * reach) as f32),
        ),
    )
}

/// The paint the stroke of `source` is drawn with, placed in the fragment's space: the shape's
/// own, or the element's.
pub(super) fn stroke_paint(
    scene: &mut Scene,
    source: &ShapeSource<'_>,
    paint: &ShapePaint,
) -> PaintRef {
    match &source.shape.stroke {
        Some(stroke) => reference(
            scene,
            &super::recognised::placed_paint(source, &stroke.paint),
            paint.stroke.unwrap_or(paint.fill),
        ),
        None => paint.stroke.map_or(PaintRef::NONE, |color| {
            PaintRef::solid(scene.paints.solid(color))
        }),
    }
}

/// The interned paint one of a document's paints becomes, given the inherited colour.
pub(super) fn reference(scene: &mut Scene, paint: &zgui_svg::Paint, inherited: Color) -> PaintRef {
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
