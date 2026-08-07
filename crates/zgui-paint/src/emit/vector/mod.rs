//! Vector content: paths, what fills them, and the one place a ramp is resolved differently.
//!
//! # Where a shape's paint comes from
//!
//! Not from `fill`. The SVG paint longhands — `fill`, `stroke`, `stroke-width` and eighteen others —
//! are gated to a different engine in this build, so a declaration using one is discarded while the
//! stylesheet is being parsed and no cascade result ever holds a value for it. Painting a shape from
//! `fill` is not something that can be made to work by reading harder.
//!
//! So the paint comes from two places that do exist.
//!
//! The default is the element's own computed `color`. That makes the `currentColor` icon the
//! default rather than a keyword, and it means `.icon:hover { color: … }` themes an icon with no
//! new mechanism and no new invalidation — `color` is already one of the groups a repaint is
//! decided on.
//!
//! An override comes from three custom properties — [`FILL`], [`STROKE`] and [`STROKE_WIDTH`] —
//! read out of the computed token streams they cascade as. That is the same route the framework
//! takes for system colours, and it works for the same reason: a custom property is a property this
//! engine build has. They inherit, so setting one on an ancestor themes every drawing below it, and
//! a change to one produces damage because the maps they live in are part of what a repaint is
//! decided on.
//!
//! # Why a ramp is resolved differently here
//!
//! A path rasteriser interpolates between the stops it is given, in sRGB, and cannot be told to walk
//! a ramp in Oklab or the long way round a hue circle. So a gradient painting vector content has its
//! ramp resolved into sRGB stops close enough together that the straight lines between them are
//! within an eight-bit step of the true curve. That resolution is the colour crate's, called from
//! here rather than written again — the same curve drawn as a box and as a path has to be the same
//! curve.

pub mod document;
pub mod fit;

use std::sync::Arc;

use zgui_color::Color;
use zgui_css::ComputedStyle;
use zgui_css::values::color::{current, to_color};
use zgui_css::values::custom;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_scene::{ClipId, Paint, PaintRef, Scene, SpatialId, VectorId, VectorItem};

use crate::content::vectors::VectorMaskSource;
use crate::emit::paint::{gradient_paint, needs_densifying};
use crate::lower::background::GradientSpec;

/// How a shape is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapePaint {
    /// What fills the shape.
    pub fill: Color,
    /// What strokes it, or nothing when it is not stroked.
    pub stroke: Option<Color>,
    /// The stroke's width in device pixels.
    pub stroke_width: f32,
}

/// The custom property that overrides what a shape is filled with.
pub const FILL: &str = "zgui-fill";

/// The custom property that says what a shape is stroked with.
pub const STROKE: &str = "zgui-stroke";

/// The custom property that says how wide that stroke is, as an absolute length.
pub const STROKE_WIDTH: &str = "zgui-stroke-width";

/// Resolves how a shape carrying `style` is painted.
///
/// The fill is [`FILL`] if the cascade produced one and the element's own `color` otherwise. There
/// is no stroke unless [`STROKE`] says there is, and its width is [`STROKE_WIDTH`] or one CSS pixel.
/// `scale` turns that width into device pixels.
///
/// A data-driven mark passes its paint as a value instead of going through any of this: a bar's
/// colour is data, and routing it through the cascade would mean a declaration written per mark per
/// frame.
pub fn shape_paint(style: &ComputedStyle, scale: f32) -> ShapePaint {
    ShapePaint {
        fill: custom::color(style, FILL).unwrap_or_else(|| to_color(current(style))),
        stroke: custom::color(style, STROKE),
        stroke_width: custom::length(style, STROKE_WIDTH).unwrap_or(1.0) * scale,
    }
}

/// Where one drawing's outlines are drawn.
#[derive(Clone, Copy, Debug)]
pub struct VectorPlacement {
    /// The chain the outlines are drawn through.
    pub clip: ClipId,
    /// The transform they are drawn under, which is the box's own.
    pub transform: SpatialId,
    /// Device pixels per CSS pixel, part of a coverage mask's raster identity.
    pub scale: f32,
}

/// Emits one path, filled and optionally stroked, and returns how many items were pushed.
///
/// `ink` is what the path covers once its stroke is accounted for. It is the caller's because the
/// caller built the path and knows its bounds; nothing here measures a Bézier.
pub fn emit(
    scene: &mut Scene,
    id: VectorId,
    path: Arc<zgui_scene::kurbo::BezPath>,
    ink: Rect<DevicePx, Device>,
    paint: ShapePaint,
    placement: VectorPlacement,
) -> usize {
    // Under the transform, because that is the rectangle the rasteriser writes: an item is drawn
    // into a scratch covering exactly its own ink and composited back through it, so an ink
    // measured in the shape's own space cuts a turned drawing off at the edge of the box it would
    // have occupied upright. The untransformed rectangle is kept beside it, because that is the
    // space the draw order is decided in.
    let local = ink;
    let ink = under(scene, placement.transform, ink);
    let mut pushed = 0;
    let fill = scene.paints.add(Paint::Solid(paint.fill));
    let mut item = VectorItem::filled(id, path.clone(), fill).clipped(placement.clip);
    item.ink = ink;
    item.local_ink = local;
    item.transform = Some(placement.transform);
    pushed += usize::from(scene.push_vector(item).is_some());
    if let Some(color) = paint.stroke {
        let stroke = scene.paints.add(Paint::Solid(color));
        let mut item =
            VectorItem::stroked(id, path, stroke, paint.stroke_width).clipped(placement.clip);
        item.ink = ink;
        item.local_ink = local;
        item.transform = Some(placement.transform);
        pushed += usize::from(scene.push_vector(item).is_some());
    }
    pushed
}

/// A rectangle in a shape's own space, as the rectangle it covers on the surface.
///
/// A transform that leaves the z = 0 plane is flattened rather than refused, which is what the
/// rasteriser does with the same matrix: what is lost is the transform, never the shape.
pub(crate) fn under(
    scene: &Scene,
    transform: SpatialId,
    rect: Rect<DevicePx, Device>,
) -> Rect<DevicePx, Device> {
    match scene
        .spatial
        .resolve(transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2)
    {
        Some(affine) => affine.transform_rect(rect),
        None => rect,
    }
}

/// Emits every shape of one drawing, and returns how many primitives were pushed.
///
/// The outlines are already in the fragment's local space — fitted to its box by [`fit::onto`]
/// before they were cached — so nothing here measures or moves a curve. Each takes its own
/// identity, derived from the fragment's and its position in the list, so a rasteriser's cached
/// encoding of one outline survives a sibling changing.
///
/// The ink of each item is measured from the outline itself, which is what makes a drawing that
/// overflows its box still put its own pixels in the damage: a rectangle taken from the box would
/// under-report exactly the overflow the box does not cover.
pub fn draw(
    scene: &mut Scene,
    base: VectorId,
    shapes: &[zgui_svg::Shape],
    paint: ShapePaint,
    placement: VectorPlacement,
) -> usize {
    draw_with_masks(
        scene,
        base,
        shapes,
        paint,
        &crate::content::NoVectorMasks,
        placement,
    )
}

/// Emits a drawing while allowing eligible solid shapes to use a monochrome mask source.
pub fn draw_with_masks(
    scene: &mut Scene,
    base: VectorId,
    shapes: &[zgui_svg::Shape],
    paint: ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> usize {
    let mut pushed = 0;
    for (index, shape) in shapes.iter().enumerate() {
        pushed += document::emit(
            scene,
            outline_id(base, index),
            shape,
            &paint,
            masks,
            placement,
        );
    }
    pushed
}

/// The identity of one outline of a drawing whose first outline is `base`.
///
/// A collision between two drawings costs a re-encoding and never a wrong picture: the rasteriser
/// holds a fingerprint of the geometry beside each cached encoding and re-encodes whenever it does
/// not match, so an identity is a hint about what is worth keeping rather than a promise about what
/// a shape is.
fn outline_id(base: VectorId, index: usize) -> VectorId {
    VectorId(
        base.0
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add((index as u32).wrapping_mul(0x85EB_CA6B)),
    )
}

/// The paint one gradient becomes when it fills vector content of `bounds`.
///
/// Unlike the same gradient on a box, a ramp outside sRGB is resolved into sRGB stops here, because
/// the rasteriser that draws it cannot walk the ramp itself. A ramp already in sRGB is passed
/// through untouched, so a two-stop gradient stays two stops.
pub fn gradient_for_vector(
    scene: &mut Scene,
    spec: &GradientSpec,
    bounds: Rect<DevicePx, Device>,
    scale: f32,
) -> Option<PaintRef> {
    let reference = gradient_paint(scene, spec, bounds, scale)?;
    if !needs_densifying(spec) {
        return Some(reference);
    }
    let id = reference.id()?;
    let Some(Paint::Gradient {
        kind,
        stops,
        repeating,
        ..
    }) = scene.paints.get(id).cloned()
    else {
        // A one-stop ramp interned as a flat colour, which needs no densifying and has no curve.
        return Some(reference);
    };
    let dense = zgui_color::densify(&stops, spec.interpolation);
    Some(scene.paints.add(Paint::Gradient {
        kind,
        stops: dense.into_iter().collect(),
        space: zgui_color::ColorSpace::Srgb,
        hue: zgui_color::HueInterpolation::Shorter,
        repeating,
    }))
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;
    use zgui_color::{Color, ColorSpace, GradientStop, Interpolation};
    use zgui_css::StyleDraft;
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    use zgui_scene::{Paint, Scene};

    use super::{gradient_for_vector, shape_paint};
    use crate::lower::background::{GradientShape, GradientSpec, SpecStop};

    /// A two-stop black-to-white ramp in the given space.
    fn ramp(space: ColorSpace) -> GradientSpec {
        GradientSpec {
            shape: GradientShape::Linear { angle: 0.0 },
            stops: smallvec![
                SpecStop {
                    color: Color::BLACK,
                    position: None,
                },
                SpecStop {
                    color: Color::WHITE,
                    position: None,
                },
            ],
            interpolation: Interpolation::new(space),
            repeating: false,
        }
    }

    /// A 100 by 100 box at the origin.
    fn bounds() -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(100.0), DevicePx(100.0)),
        )
    }

    /// The stops a paint reference resolves to.
    fn stops_of(scene: &Scene, reference: zgui_scene::PaintRef) -> Vec<GradientStop> {
        match scene.paints.get(reference.id().expect("a paint")) {
            Some(Paint::Gradient { stops, .. }) => stops.to_vec(),
            other => panic!("expected a gradient, found {other:?}"),
        }
    }

    #[test]
    fn a_shape_with_no_paint_of_its_own_is_filled_with_the_elements_colour() {
        let style = StyleDraft::initial().build();
        let paint = shape_paint(&style, 1.0);
        assert_eq!(paint.fill.to_premultiplied_srgb(), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(paint.stroke, None);
        assert_eq!(shape_paint(&style, 2.0).stroke_width, 2.0);
    }

    #[test]
    fn an_srgb_ramp_reaches_a_rasteriser_with_the_stops_it_was_written_with() {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(100, 100));
        let reference = gradient_for_vector(&mut scene, &ramp(ColorSpace::Srgb), bounds(), 1.0)
            .expect("a ramp");
        assert_eq!(stops_of(&scene, reference).len(), 2);
    }

    #[test]
    fn an_oklab_ramp_is_resolved_into_srgb_stops_a_rasteriser_can_interpolate() {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(100, 100));
        let reference = gradient_for_vector(&mut scene, &ramp(ColorSpace::Oklab), bounds(), 1.0)
            .expect("a ramp");
        let stops = stops_of(&scene, reference);
        assert!(
            stops.len() > 2,
            "a curve that is not a straight line in sRGB needs more than its endpoints"
        );
        assert!(
            stops
                .iter()
                .all(|stop| stop.color.space() == ColorSpace::Srgb),
            "the whole point is that every stop is one the rasteriser can blend between"
        );
        assert!(
            stops
                .windows(2)
                .all(|pair| pair[0].offset <= pair[1].offset),
            "densifying must not reorder the ramp"
        );
    }
}
