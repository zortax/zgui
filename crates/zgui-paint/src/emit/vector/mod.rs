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
mod path_glyphs;
pub(crate) mod recognise;
pub(crate) mod recognised;
mod series;
pub(crate) mod split;

pub(crate) use series::shown;

use std::sync::OnceLock;

use zgui_color::Color;
use zgui_css::ComputedStyle;
use zgui_css::values::color::{current, to_color};
use zgui_css::values::custom;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_profile::{Counter, counter};
use zgui_scene::{ClipId, ClipLink, Paint, PaintRef, Scene, SpatialId, VectorId};

use crate::content::vectors::VectorMaskSource;
use crate::emit::paint::{gradient_paint, needs_densifying};
use crate::emit::record::Recorder;
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

/// The custom property that reduces a drawing's long line series per device column.
///
/// `--zgui-vector-lod: columns` draws each line series whose points run left to right, with more
/// than four points per device column, through the first, lowest, highest and last point of each
/// column. Shapes are always drawn exactly.
pub const LOD: &str = "zgui-vector-lod";

/// Whether `style` asks for line series reduced per device column.
pub fn vector_lod(style: &ComputedStyle) -> bool {
    custom::text(style, LOD).is_some_and(|value| value.trim() == "columns")
}

/// The item kind selected for one vector shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorRoute {
    /// A path item.
    GeneralRaster,
    /// Rounded, bordered quads, one per recognised circle, ellipse, rectangle or stroke segment.
    Analytic,
    /// One marks item per part, drawing every recognised prim of the part from a shared payload.
    Marks,
    /// The whole drawing rasterised on the CPU into images.
    CpuLayer,
    /// One marks item per part, placing every subpath as a copy of its repeated outline.
    PathGlyphs,
}

/// The raster paths used by all shapes belonging to one element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VectorRoutes(u8);

impl VectorRoutes {
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
    /// How many items were recorded.
    pub(crate) pushed: usize,
    /// Which route the shape selected, if it drew anything.
    pub(crate) route: Option<VectorRoute>,
}

/// What emitting all shapes belonging to one element did.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DrawingEmission {
    /// How many items were recorded.
    pub(crate) pushed: usize,
    /// Every route selected by the drawing's shapes.
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
    /// The folded opacity, which the layer's images carry.
    pub alpha: f32,
}

/// What the layer route did for one drawing.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LayerOutcome {
    /// Whether a layer image was recorded.
    pub(crate) sprite: bool,
    /// Whether that image stretches a raster of another scale or phase.
    pub(crate) provisional: bool,
    /// Whether the drawing drew nothing, to be drawn on a later frame.
    pub(crate) deferred: bool,
    /// Whether the drawing fell back to its shapes for the budget or a demotion, and may take a
    /// layer later.
    pub(crate) promote: bool,
    /// The device pixels a provisional or deferred drawing is owed a frame for.
    pub(crate) owed: Option<Rect<i32, Device>>,
    /// The number of the tiled source whose tiles the drawing recorded.
    pub(crate) tiled: Option<u64>,
    /// The tile images the drawing recorded, with their rectangles in the fragment's space.
    pub(crate) tiles: Option<crate::emit::record::ShownTiles>,
    /// The region of local space whose prims a series cut to it draws.
    pub(crate) window: Option<zgui_scene::kurbo::Rect>,
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

/// Records every shape of one drawing, and returns how many items were recorded.
///
/// The outlines are already in the fragment's local space. Each takes its own identity, derived
/// from `base` and its position in the list, which its route history is kept by.
pub fn draw(
    rec: &mut Recorder<'_>,
    base: VectorId,
    shapes: &[zgui_svg::Shape],
    paint: ShapePaint,
    placement: VectorPlacement,
) -> usize {
    draw_with_masks(
        rec,
        base,
        shapes,
        paint,
        &crate::content::NoVectorMasks,
        placement,
    )
}

/// Records a drawing with the route histories and caches of `masks`.
pub fn draw_with_masks(
    rec: &mut Recorder<'_>,
    base: VectorId,
    shapes: &[zgui_svg::Shape],
    paint: ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> usize {
    draw_with_masks_tracked(rec, base, shapes, paint, masks, placement).pushed
}

/// Records a drawing and notes every route its shapes selected.
pub(crate) fn draw_with_masks_tracked(
    rec: &mut Recorder<'_>,
    base: VectorId,
    shapes: &[zgui_svg::Shape],
    paint: ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> DrawingEmission {
    let mut emitted = DrawingEmission::default();
    for (index, shape) in shapes.iter().enumerate() {
        let shape = document::emit_tracked(
            rec,
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

/// Records every shape and series of a drawing from its source, and returns how many items were
/// recorded.
pub fn draw_drawing(
    rec: &mut Recorder<'_>,
    base: VectorId,
    drawing: &crate::content::Drawing,
    paint: ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> usize {
    draw_drawing_tracked(rec, base, drawing, paint, masks, placement, None, false).pushed
}

/// The same as [`draw_drawing`], letting a candidate drawing take the layer route of `masks`.
#[doc(hidden)]
pub fn draw_drawing_layered(
    rec: &mut Recorder<'_>,
    base: VectorId,
    drawing: &crate::content::Drawing,
    paint: ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
    layer: LayerInput,
) -> usize {
    draw_drawing_tracked(
        rec,
        base,
        drawing,
        paint,
        masks,
        placement,
        Some(layer),
        false,
    )
    .pushed
}

/// Records one CPU layer raster as an image over `local`, through the placement's clip and
/// transform, at `alpha`, and returns how many items were recorded.
///
/// The image is cut to a frame one unit wider than `local` on every side, so the frame's soft
/// edge falls outside the image. The raster carries its own coverage.
#[doc(hidden)]
pub fn layer_image(
    rec: &mut Recorder<'_>,
    local: Rect<DevicePx, Device>,
    raster: &crate::content::vectors::LayerPixels,
    placement: VectorPlacement,
    alpha: f32,
) -> usize {
    let Some((image, _)) = rec.image_slot(raster.key, Some(raster.pixels())) else {
        return 0;
    };
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
    let item = compositor_types::Image {
        bounds: crate::emit::record::local_rect(local),
        frame: crate::emit::record::local_rect(frame),
        source: [0.0, 0.0, raster.width as f32, raster.height as f32],
        radii: [0.0; 8],
        shapes: compositor_types::ROUND,
        paint: compositor_types::PaintRef::NONE,
        image,
        opacity: alpha,
        clip: rec.clip(placement.clip),
        space: rec.space(placement.transform),
        flags: compositor_types::ImageFlags::default(),
        extend: [compositor_types::Extend::NONE; 2],
        filter: compositor_types::ImageFilter::LINEAR,
    };
    rec.image(item);
    1
}

/// Notes that the tile image `key` is shown over `local` in the fragment's space, as far as the
/// placement's clip admits it on the surface.
pub(crate) fn show_tile(
    layers: &dyn crate::content::vectors::VectorLayerSource,
    scene: &Scene,
    placement: VectorPlacement,
    key: crate::emit::record::ImageKey,
    local: Rect<DevicePx, Device>,
) {
    let device = under(scene, placement.transform, local);
    let admitted = scene
        .clips
        .bounds_placed(placement.clip, &|space| scene.spatial.resolve(space));
    layers.shown(key, device.intersection(admitted));
}

/// Returns the clip chain of `placement` narrowed by `links`, as the reference items drawn
/// through it hold.
///
/// The links are chunk-local clips, so they move with the chunk. A link in another coordinate
/// system than the chunk's is a clip chain the scene mints.
pub(crate) fn clip_through(
    rec: &mut Recorder<'_>,
    placement: VectorPlacement,
    links: impl IntoIterator<Item = ClipLink>,
) -> compositor_types::ClipRef {
    let mut chain = placement.clip;
    let mut clip = rec.clip(chain);
    for link in links {
        if let Some(local) = rec.local_clip_in(clip, link) {
            clip = local;
            continue;
        }
        let scene = rec.scene();
        chain = scene.clips.push(chain, link);
        scene.note_minted_clip(chain);
        clip = rec.clip(chain);
    }
    clip
}

/// Whether a drawing may take the layer route: it has no series, and some shape with a gradient
/// or a clip would take the general route.
///
/// A shape the analytic or the marks route takes is exact at every scale and costs no raster, so a
/// drawing whose every such shape goes there stays on its shapes. The question is asked without
/// recording anything, and the recognitions it makes are kept for the shapes' own emission.
fn candidate(
    rec: &mut Recorder<'_>,
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
    let affine = rec
        .scene_ref()
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
            analytic::emit_analytic(rec, id, &source, paint, masks, placement, affine, true);
        analytic.is_none()
            && marks::emit_marks(rec, id, &source, paint, masks, placement, affine, true).is_none()
    })
}

/// Records every shape and series of a drawing from its source, and notes every route selected.
///
/// No route places a shape, except recognition under a fit that scales its axes differently. A
/// series is drawn before the shape its position names, and after the last shape when it names
/// none.
///
/// With `layer`, a candidate drawing asks the layer route first. A layer is one image, or one per
/// tile, carrying the box's clip and transform and the folded opacity, and replaces every shape.
///
/// With `lod`, every line series is reduced per device column, as [`LOD`] says.
#[expect(
    clippy::too_many_arguments,
    reason = "the drawing, where it goes and the two routes it may ask for"
)]
pub(crate) fn draw_drawing_tracked(
    rec: &mut Recorder<'_>,
    base: VectorId,
    drawing: &crate::content::Drawing,
    paint: ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
    layer: Option<LayerInput>,
    lod: bool,
) -> DrawingEmission {
    let mut emitted = DrawingEmission::default();
    let layers = layer
        .zip(masks.layers())
        .filter(|_| candidate(rec, base, drawing, &paint, masks, placement));
    if let Some((input, layers)) = layers {
        let spatial = rec
            .scene_ref()
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
                raster,
                local,
                provisional,
            } => {
                emitted.pushed += layer_image(rec, local, &raster, placement, input.alpha);
                emitted.routes.insert(VectorRoute::CpuLayer);
                emitted.layer.sprite = true;
                counter::bump(Counter::VectorRouteLayer);
                if provisional {
                    counter::bump(Counter::VectorLayersProvisional);
                    emitted.layer.provisional = true;
                    emitted.layer.owed = owed(rec.scene_ref(), local);
                }
                return emitted;
            }
            crate::content::vectors::LayerAnswer::Tiles { tiles, provisional } => {
                // Each tile is an image of its raster. The end of the walk rasterises the ones the
                // frame needs and writes them.
                let mut shown = Vec::with_capacity(tiles.len());
                for tile in tiles.tiles() {
                    let local = rect_of(drawing.fit.transform_rect_bbox(tile.path_rect));
                    emitted.pushed += layer_image(rec, local, &tile.pixels, placement, input.alpha);
                    show_tile(layers, rec.scene_ref(), placement, tile.pixels.key, local);
                    shown.push((tile.pixels.key, local));
                }
                emitted.layer.tiles = Some(shown.into());
                emitted.routes.insert(VectorRoute::CpuLayer);
                emitted.layer.tiled = Some(tiles.id());
                counter::bump(Counter::VectorRouteLayer);
                if provisional {
                    counter::bump(Counter::VectorLayersProvisional);
                    emitted.layer.provisional = true;
                    let ink = tiles
                        .tiles()
                        .iter()
                        .map(|tile| drawing.fit.transform_rect_bbox(tile.path_rect))
                        .reduce(|a, b| a.union(b));
                    emitted.layer.owed = ink.and_then(|ink| owed(rec.scene_ref(), rect_of(ink)));
                }
                return emitted;
            }
            crate::content::vectors::LayerAnswer::Defer { local } => {
                counter::bump(Counter::VectorLayersDeferred);
                emitted.routes.insert(VectorRoute::CpuLayer);
                emitted.layer.deferred = true;
                emitted.layer.owed = owed(rec.scene_ref(), local);
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
    let note = |emitted: &mut DrawingEmission, shape: ShapeEmission| {
        emitted.pushed += shape.pushed;
        if let Some(route) = shape.route {
            emitted.routes.insert(route);
        }
    };
    // A series drawn from a provisional payload is owed a frame, as a provisional layer is. A
    // series cut to a window replays only while the window holds what is shown.
    let series_note =
        |emitted: &mut DrawingEmission,
         (shape, outcome): (ShapeEmission, series::SeriesOutcome)| {
            note(emitted, shape);
            if let Some(owed) = outcome.owed {
                emitted.layer.provisional = true;
                emitted.layer.owed = series::union(emitted.layer.owed, owed);
            }
            if outcome.window.is_some() {
                emitted.layer.window = series::meet(emitted.layer.window, outcome.window);
            }
        };
    // A series that draws through the general route takes an identity past every shape's.
    let mut series = drawing
        .series
        .iter()
        .enumerate()
        .map(|(index, at)| (outline_id(base, drawing.shapes.len() + index), at))
        .peekable();
    for index in 0..drawing.shapes.len() {
        while let Some((id, at)) = series.next_if(|(_, at)| at.before <= index) {
            series_note(
                &mut emitted,
                series::emit_series(
                    rec,
                    id,
                    &at.series,
                    drawing.fit,
                    &paint,
                    masks,
                    placement,
                    lod || at.lod == zgui_canvas::Lod::Columns,
                ),
            );
        }
        note(
            &mut emitted,
            document::emit_tracked(
                rec,
                outline_id(base, index),
                &ShapeSource::of(drawing, index),
                &paint,
                masks,
                placement,
            ),
        );
    }
    for (id, at) in series {
        series_note(
            &mut emitted,
            series::emit_series(
                rec,
                id,
                &at.series,
                drawing.fit,
                &paint,
                masks,
                placement,
                lod || at.lod == zgui_canvas::Lod::Columns,
            ),
        );
    }
    // A candidate whose shapes all found a route of their own needs no layer.
    if let Some((input, layers)) = layers
        && !emitted.routes.contains(VectorRoute::GeneralRaster)
    {
        layers.per_shape_suffices(input.owner);
    }
    emitted
}

/// A rectangle of path-space arithmetic in the fragment's space.
fn rect_of(rect: zgui_scene::kurbo::Rect) -> Rect<DevicePx, Device> {
    Rect::new(
        zgui_geom::Point::new(DevicePx(rect.x0 as f32), DevicePx(rect.y0 as f32)),
        zgui_geom::Size::new(
            DevicePx(rect.width() as f32),
            DevicePx(rect.height() as f32),
        ),
    )
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
}
