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

mod analytic;
pub mod document;
pub mod fit;
mod marks;
pub(crate) mod recognise;
pub(crate) mod recognised;
mod series;

use std::sync::{Arc, OnceLock};

use zgui_color::Color;
use zgui_css::ComputedStyle;
use zgui_css::values::color::{current, to_color};
use zgui_css::values::custom;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_profile::{Counter, counter};
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

/// The raster path selected for one vector shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorRoute {
    /// A CPU-rasterised monochrome mask stored in the ordinary atlas.
    AtlasMask,
    /// A general vector item, executed by the renderer's configured vector rasteriser.
    GeneralRaster,
    /// Rounded, bordered quads, one per recognised circle, ellipse, rectangle or stroke segment.
    Analytic,
    /// One mark per part, drawing every recognised prim of the part from a shared payload.
    Marks,
    /// The whole drawing rasterised on the CPU into one colour tile, drawn as one sprite.
    CpuLayer,
    /// One mark per part, drawing every subpath from atlas cells of its repeated outline.
    PathGlyphs,
}

/// The raster paths used by all shapes belonging to one element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VectorRoutes(u8);

impl VectorRoutes {
    const ATLAS_MASK: u8 = 1;
    const GENERAL_RASTER: u8 = 2;
    const ANALYTIC: u8 = 4;
    const MARKS: u8 = 8;
    const PATH_GLYPHS: u8 = 16;
    const CPU_LAYER: u8 = 32;

    /// No vector shape was emitted for the element.
    pub const NONE: Self = Self(0);

    /// Adds `route` to this set.
    pub fn insert(&mut self, route: VectorRoute) {
        self.0 |= Self::bit(route);
    }

    /// Adds every route in `other` to this set.
    pub fn union_with(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Whether at least one shape used `route`.
    pub const fn contains(self, route: VectorRoute) -> bool {
        self.0 & Self::bit(route) != 0
    }

    /// The bit that stands for `route`.
    const fn bit(route: VectorRoute) -> u8 {
        match route {
            VectorRoute::AtlasMask => Self::ATLAS_MASK,
            VectorRoute::GeneralRaster => Self::GENERAL_RASTER,
            VectorRoute::Analytic => Self::ANALYTIC,
            VectorRoute::Marks => Self::MARKS,
            VectorRoute::PathGlyphs => Self::PATH_GLYPHS,
            VectorRoute::CpuLayer => Self::CPU_LAYER,
        }
    }

    /// Whether no vector route is represented.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// One shape as a drawing holds it: in its own space, with the fit that places it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShapeSource<'a> {
    /// The shape in its own space.
    pub(crate) shape: &'a zgui_svg::Shape,
    /// What places it in the fragment's space.
    pub(crate) fit: zgui_scene::kurbo::Affine,
    /// Where the placed shape is kept once made, for a fit that is not the identity.
    pub(crate) cell: Option<&'a OnceLock<zgui_svg::Shape>>,
}

impl<'a> ShapeSource<'a> {
    /// A shape already in the fragment's space.
    pub(crate) fn placed_shape(shape: &'a zgui_svg::Shape) -> Self {
        Self {
            shape,
            fit: zgui_scene::kurbo::Affine::IDENTITY,
            cell: None,
        }
    }

    /// A shape in the painter's coordinates, moved by `offset` into the fragment's space.
    ///
    /// A translation is a uniform fit, so no route places the shape.
    pub(crate) fn translated(shape: &'a zgui_svg::Shape, offset: zgui_scene::kurbo::Vec2) -> Self {
        Self {
            shape,
            fit: zgui_scene::kurbo::Affine::translate(offset),
            cell: None,
        }
    }

    /// Shape `index` of `drawing`.
    pub(crate) fn of(drawing: &'a crate::content::Drawing, index: usize) -> Self {
        Self {
            shape: &drawing.shapes[index],
            fit: drawing.fit,
            cell: drawing.cell(index),
        }
    }

    /// The shape in the fragment's space, placed on first use.
    ///
    /// Only recognition under a fit that scales its axes differently reads it.
    pub(crate) fn placed(&self) -> &'a zgui_svg::Shape {
        if self.fit == zgui_scene::kurbo::Affine::IDENTITY {
            return self.shape;
        }
        let fit = self.fit;
        let shape = self.shape;
        match self.cell {
            Some(cell) => cell.get_or_init(|| zgui_svg::document::place::shape(shape, fit)),
            None => {
                debug_assert!(false, "a fitted shape has a cell to be placed into");
                shape
            }
        }
    }
}

/// What emitting one shape did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ShapeEmission {
    /// How many primitives survived insertion into the scene.
    pub(crate) pushed: usize,
    /// Which raster path the shape selected, if it drew anything.
    pub(crate) route: Option<VectorRoute>,
}

/// What emitting all shapes belonging to one element did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DrawingEmission {
    /// How many primitives survived insertion into the scene.
    pub(crate) pushed: usize,
    /// Every raster path selected by the drawing's shapes.
    pub(crate) routes: VectorRoutes,
    /// What the layer route did.
    pub(crate) layer: LayerOutcome,
}

/// What a drawing asks of the layer route, beside its shapes.
#[derive(Clone, Copy, Debug)]
pub struct LayerInput {
    /// The drawing's element, which its layer history is kept by.
    pub owner: VectorId,
    /// The revision of the drawing's source.
    pub revision: u64,
    /// What an inherited paint resolves to, before any folded opacity.
    pub paint: ShapePaint,
    /// The folded opacity, which the sprite carries.
    pub alpha: f32,
}

/// What the layer route did for one drawing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LayerOutcome {
    /// Whether a layer sprite was pushed.
    pub(crate) sprite: bool,
    /// Whether that sprite stretches a raster of another scale or phase.
    pub(crate) provisional: bool,
    /// Whether the drawing drew nothing, to be drawn on a later frame.
    pub(crate) deferred: bool,
    /// Whether the drawing fell back to its shapes for the budget or a demotion, and may take a
    /// layer later.
    pub(crate) promote: bool,
    /// The device pixels a provisional or deferred drawing is owed a frame for.
    pub(crate) owed: Option<Rect<i32, Device>>,
}

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
/// The outlines are already in the fragment's local space, so nothing here measures or moves a
/// curve. Each takes its own
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
    draw_with_masks_tracked(scene, base, shapes, paint, masks, placement).pushed
}

/// Emits a drawing and records all raster paths selected by its shapes.
pub(crate) fn draw_with_masks_tracked(
    scene: &mut Scene,
    base: VectorId,
    shapes: &[zgui_svg::Shape],
    paint: ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> DrawingEmission {
    let mut emitted = DrawingEmission::default();
    for (index, shape) in shapes.iter().enumerate() {
        let shape = document::emit_tracked(
            scene,
            outline_id(base, index),
            &ShapeSource::placed_shape(shape),
            &paint,
            masks,
            placement,
        );
        emitted.pushed += shape.pushed;
        if let Some(route) = shape.route {
            emitted.routes.insert(route);
        }
    }
    emitted
}

/// Emits every shape and series of a drawing from its source, and returns how many primitives
/// were pushed.
pub fn draw_drawing(
    scene: &mut Scene,
    base: VectorId,
    drawing: &crate::content::Drawing,
    paint: ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> usize {
    draw_drawing_tracked(scene, base, drawing, paint, masks, placement, None).pushed
}

/// The same as [`draw_drawing`], letting a candidate drawing take the layer route of `masks`.
#[doc(hidden)]
pub fn draw_drawing_layered(
    scene: &mut Scene,
    base: VectorId,
    drawing: &crate::content::Drawing,
    paint: ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
    layer: LayerInput,
) -> usize {
    draw_drawing_tracked(scene, base, drawing, paint, masks, placement, Some(layer)).pushed
}

/// The sprite one CPU layer is drawn as: the tile at `local`, through the placement's clip and
/// transform, at `alpha`.
///
/// The frame is wider than the sprite by one unit on every side, so the frame's soft edge falls
/// outside the quad. The quad's own edge lands on whole device pixels and the raster carries its
/// own coverage.
#[doc(hidden)]
pub fn layer_sprite(
    local: Rect<DevicePx, Device>,
    tile: zgui_atlas::AtlasTile,
    placement: VectorPlacement,
    alpha: f32,
) -> zgui_scene::ColorSprite {
    let frame = Rect::new(
        zgui_geom::Point::new(
            DevicePx(local.origin.x.0 - 1.0),
            DevicePx(local.origin.y.0 - 1.0),
        ),
        zgui_geom::Size::new(
            DevicePx(local.size.width.0 + 2.0),
            DevicePx(local.size.height.0 + 2.0),
        ),
    );
    let mut sprite = zgui_scene::ColorSprite::new(local, tile)
        .framed(frame)
        .clipped(placement.clip);
    sprite.transform = placement.transform.index();
    sprite.opacity = alpha;
    sprite
}

/// Whether a drawing may take the layer route: it has no series, and some shape with a gradient
/// or a clip would take the general route.
///
/// A shape the analytic or the marks route takes is exact at every scale and costs no raster, so a
/// drawing whose every such shape goes there stays on its shapes. The question is asked without
/// pushing anything, and the recognitions it makes are kept for the shapes' own emission.
fn candidate(
    scene: &mut Scene,
    base: VectorId,
    drawing: &crate::content::Drawing,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> bool {
    if !drawing.series.is_empty() {
        return false;
    }
    let gradient = |paint: &zgui_svg::Paint| matches!(paint, zgui_svg::Paint::Gradient(_));
    let affine = scene
        .spatial
        .resolve(placement.transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2);
    drawing.shapes.iter().enumerate().any(|(index, shape)| {
        let general = !shape.clips.is_empty()
            || shape
                .fill
                .as_ref()
                .is_some_and(|fill| gradient(&fill.paint))
            || shape
                .stroke
                .as_ref()
                .is_some_and(|stroke| gradient(&stroke.paint));
        if !general {
            return false;
        }
        let id = outline_id(base, index);
        let source = ShapeSource::of(drawing, index);
        let analytic =
            analytic::emit_analytic(scene, id, &source, paint, masks, placement, affine, true);
        analytic.is_none()
            && marks::emit_marks(scene, id, &source, paint, masks, placement, affine, true)
                .is_none()
    })
}

/// Emits every shape and series of a drawing from its source, and records all raster paths
/// selected.
///
/// No route places a shape, except recognition under a fit that scales its axes differently. A
/// series is drawn before the shape its position names, and after the last shape when it names
/// none.
///
/// With `layer`, a candidate drawing asks the layer route first. A layer is one colour sprite
/// carrying the box's clip and transform and the folded opacity, and replaces every shape.
pub(crate) fn draw_drawing_tracked(
    scene: &mut Scene,
    base: VectorId,
    drawing: &crate::content::Drawing,
    paint: ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
    layer: Option<LayerInput>,
) -> DrawingEmission {
    let mut emitted = DrawingEmission::default();
    let layers = layer
        .zip(masks.layers())
        .filter(|_| candidate(scene, base, drawing, &paint, masks, placement));
    if let Some((input, layers)) = layers {
        let spatial = scene
            .spatial
            .resolve(placement.transform)
            .as_ref()
            .and_then(zgui_geom::Matrix4::to_affine2);
        let answer = layers.layer(crate::content::vectors::LayerRequest {
            owner: input.owner,
            revision: input.revision,
            drawing,
            paint: input.paint,
            spatial,
        });
        // The ink under the transform, rounded out: what a provisional or deferred drawing is owed.
        let owed = |scene: &Scene, local: Rect<DevicePx, Device>| {
            let ink = under(scene, placement.transform, local);
            let left = ink.left().0.floor();
            let top = ink.top().0.floor();
            let right = ink.right().0.ceil();
            let bottom = ink.bottom().0.ceil();
            [left, top, right, bottom]
                .iter()
                .all(|edge| edge.is_finite())
                .then(|| {
                    Rect::new(
                        zgui_geom::Point::new(left as i32, top as i32),
                        zgui_geom::Size::new((right - left) as i32, (bottom - top) as i32),
                    )
                })
        };
        match answer {
            crate::content::vectors::LayerAnswer::Sprite {
                tile,
                local,
                provisional,
                ..
            } => {
                let sprite = layer_sprite(local, tile, placement, input.alpha);
                emitted.pushed += usize::from(scene.push_color_sprite(sprite).is_some());
                emitted.routes.insert(VectorRoute::CpuLayer);
                emitted.layer.sprite = true;
                counter::bump(Counter::VectorRouteLayer);
                if provisional {
                    counter::bump(Counter::VectorLayersProvisional);
                    emitted.layer.provisional = true;
                    emitted.layer.owed = owed(scene, local);
                }
                return emitted;
            }
            crate::content::vectors::LayerAnswer::Defer { local } => {
                counter::bump(Counter::VectorLayersDeferred);
                emitted.routes.insert(VectorRoute::CpuLayer);
                emitted.layer.deferred = true;
                emitted.layer.owed = owed(scene, local);
                return emitted;
            }
            crate::content::vectors::LayerAnswer::Items(why) => {
                use crate::content::vectors::LayerFallback;
                if why != LayerFallback::PerShape {
                    counter::bump(Counter::VectorLayerFallbacks);
                }
                emitted.layer.promote =
                    matches!(why, LayerFallback::Budget | LayerFallback::Demoted);
            }
        }
    }
    let mut note = |shape: ShapeEmission| {
        emitted.pushed += shape.pushed;
        if let Some(route) = shape.route {
            emitted.routes.insert(route);
        }
    };
    let mut series = drawing.series.iter().peekable();
    for index in 0..drawing.shapes.len() {
        while let Some(at) = series.next_if(|at| at.before <= index) {
            note(series::emit_series(
                scene,
                &at.series,
                drawing.fit,
                &paint,
                masks,
                placement,
            ));
        }
        note(document::emit_tracked(
            scene,
            outline_id(base, index),
            &ShapeSource::of(drawing, index),
            &paint,
            masks,
            placement,
        ));
    }
    for at in series {
        note(series::emit_series(
            scene,
            &at.series,
            drawing.fit,
            &paint,
            masks,
            placement,
        ));
    }
    // A candidate whose shapes all found a route of their own needs no layer.
    if let Some((input, layers)) = layers
        && !emitted.routes.contains(VectorRoute::GeneralRaster)
    {
        layers.per_shape_suffices(input.owner);
    }
    emitted
}

/// The identity of one outline of a drawing whose first outline is `base`.
///
/// A collision between two drawings costs nothing in the picture: the rasteriser keys its
/// encodings on content, and the mask route keys its tiles on the raster. An identity names an
/// owner for the mask route's history, and it is no promise about what a shape is.
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

    /// A mask source over a real mask cache that records each stroke width it is asked for.
    struct Recording {
        cache: core::cell::RefCell<crate::content::vectors::VectorMaskCache>,
        atlas: core::cell::RefCell<zgui_atlas::Atlas>,
        asked: core::cell::RefCell<Vec<(f64, zgui_atlas::AtlasKey)>>,
    }

    impl crate::content::vectors::VectorMaskSource for Recording {
        fn vector_mask(
            &self,
            request: crate::content::vectors::VectorMaskRequest<'_>,
        ) -> Option<crate::content::vectors::VectorMask> {
            let mask = self
                .cache
                .borrow_mut()
                .tile_for(&mut self.atlas.borrow_mut(), request)?;
            if let crate::content::vectors::VectorMaskStyle::Stroke(style) = request.style {
                self.asked.borrow_mut().push((style.width, mask.key));
            }
            Some(mask)
        }

        fn analytic(&self, _owner: zgui_scene::VectorId) -> bool {
            false
        }

        fn marks(&self, _owner: zgui_scene::VectorId) -> bool {
            false
        }
    }

    /// The mask route scales a shape's own stroke by the fit, so the shape through the fit is the
    /// same mask as the shape placed by that fit.
    #[test]
    fn a_shapes_own_stroke_is_scaled_by_the_fit_on_the_mask_route() {
        use std::sync::Arc;

        use zgui_scene::kurbo::{self, Affine, BezPath};
        use zgui_scene::{ClipId, SpatialId, VectorId};

        use super::document::emit_tracked;
        use super::{ShapeSource, VectorPlacement, VectorRoute};

        let mut path = BezPath::new();
        path.move_to((2.0, 2.0));
        path.line_to((20.0, 4.0));
        path.line_to((6.0, 18.0));
        let shape = zgui_svg::Shape {
            path: Arc::new(path),
            fill: None,
            stroke: Some(zgui_svg::Stroke {
                paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Inherited { alpha: 1.0 }),
                style: kurbo::Stroke::new(2.0),
            }),
            clips: Vec::new(),
        };
        let fit = Affine::translate((3.0, 2.0)) * Affine::scale(2.0);
        let placed = zgui_svg::document::place::shape(&shape, fit);

        let masks = Recording {
            cache: Default::default(),
            atlas: core::cell::RefCell::new(zgui_atlas::Atlas::new(
                zgui_atlas::AtlasLimits::default(),
            )),
            asked: Default::default(),
        };
        masks.atlas.borrow_mut().begin_frame();
        masks.cache.borrow_mut().begin_frame();
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(100, 100));
        let paint = shape_paint(&StyleDraft::initial().build(), 1.0);
        let placement = VectorPlacement {
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
            scale: 1.0,
        };
        let through = ShapeSource {
            shape: &shape,
            fit,
            cell: None,
        };
        for (owner, source) in [
            (VectorId(1), through),
            (VectorId(2), ShapeSource::placed_shape(&placed)),
        ] {
            let emitted = emit_tracked(&mut scene, owner, &source, &paint, &masks, placement);
            assert_eq!(emitted.route, Some(VectorRoute::AtlasMask));
        }

        let asked = masks.asked.borrow();
        assert_eq!(
            asked.iter().map(|(width, _)| *width).collect::<Vec<_>>(),
            [4.0, 4.0],
            "a 2 unit stroke under a fit of 2 is 4 pixels wide"
        );
        assert_eq!(asked[0].1, asked[1].1, "one raster, one atlas entry");
        let sprites = &scene.primitives.mono_sprites;
        assert_eq!(sprites.len(), 2);
        assert_eq!(sprites[0].ink(), sprites[1].ink());
    }
}
