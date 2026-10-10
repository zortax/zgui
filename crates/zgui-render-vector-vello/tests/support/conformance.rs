//! One list of shapes drawn through a recognising route and through the general route, and both
//! measured against the true coverage.
//!
//! Where the path renderer paints one flat colour the two pictures have to be equal. Along an edge
//! they may differ by the antialiasing rule: the analytic shaders ramp coverage over one pixel of
//! distance, and the path renderer measures area. A third picture stands for the true coverage:
//! the shapes flattened to lines within a two-hundredth of a pixel, strokes replaced by their
//! outlines, drawn by the path renderer, which is exact on straight edges.
//!
//! The path glyph route rasterises into a content cache, whose tiles are flushed to the renderer's
//! atlas before the picture is drawn.

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::Affine2;
use zgui_paint::content::vectors::{GlyphRequest, GlyphSheets, MarkPayloads, Splits};
use zgui_paint::content::{
    AnalyticOnly, MarksOnly, VectorMask, VectorMaskRequest, VectorMaskSource,
};
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement, draw, draw_with_masks};
use zgui_paint::{ContentCache, FrameContent};
use zgui_render_wgpu::{Pixels, WgpuRenderer};
use zgui_scene::kurbo::{self, BezPath};
use zgui_scene::{ClipId, OwnSpace, PropertyOwner, Scene, SpatialId, VectorId, peniko};
use zgui_svg::{Fill, Shape};

use super::{SIDE, Which, harness, opaque, present, quad, rect};

/// The largest mean difference along the edges, in levels of 255.
pub(crate) const MEAN: f64 = 2.0;

/// The largest difference at one edge pixel: the seam's tolerance between two rasterisers.
pub(crate) const WORST: u8 = 72;

/// Which recognising route a picture is allowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    /// Analytic quads.
    Analytic,
    /// Marks.
    Marks,
    /// Path glyphs.
    Glyphs,
}

/// A source of positioned glyphs with nothing in it.
pub(crate) struct NoShaped;

impl zgui_text::ShapedGlyphs for NoShaped {
    fn visit_line(
        &self,
        _paragraph: zgui_text::ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
    ) {
    }
}

/// A frame of a content cache that allows the path glyph route alone, and keeps mark payloads.
pub(crate) struct GlyphsOnly<'a>(pub(crate) &'a FrameContent<'a>);

impl VectorMaskSource for GlyphsOnly<'_> {
    fn vector_mask(&self, _request: VectorMaskRequest<'_>) -> Option<VectorMask> {
        None
    }

    fn analytic(&self, _owner: VectorId) -> bool {
        false
    }

    fn marks(&self, _owner: VectorId) -> bool {
        false
    }

    fn payloads(&self) -> Option<core::cell::RefMut<'_, MarkPayloads>> {
        self.0.payloads()
    }

    fn path_glyphs(&self, owner: VectorId) -> bool {
        self.0.path_glyphs(owner)
    }

    fn path_glyphs_declined(&self, owner: VectorId) {
        self.0.path_glyphs_declined(owner);
    }

    fn glyph_splits(&self) -> Option<core::cell::RefMut<'_, Splits>> {
        self.0.glyph_splits()
    }

    fn glyph_sheets(&self, request: GlyphRequest<'_>) -> Option<GlyphSheets> {
        self.0.glyph_sheets(request)
    }
}

/// Runs `draw` over a frame of `content` that allows the path glyph route alone, then flushes the
/// frame's tiles to `renderer`'s atlas.
pub(crate) fn with_glyphs<R>(
    content: &mut ContentCache,
    renderer: &mut WgpuRenderer,
    draw: impl FnOnce(&dyn VectorMaskSource) -> R,
) -> R {
    let store = zgui_layout::tree::store::LayoutStore::new(zgui_arena::DocumentId::FIRST);
    content.set_vector_raster_ready(true);
    content.begin_frame();
    let out = {
        let frame = content.frame(&store, &NoShaped, &zgui_text::NoRaster);
        draw(&GlyphsOnly(&frame))
    };
    content
        .flush(renderer.atlas())
        .expect("the device accepts the tiles");
    content.end_frame();
    out
}

/// `shapes` on black under `placement`, drawn with `route` allowed, or through the general route
/// alone. The glyph route draws through `glyphs`.
pub(crate) fn scene_of(shapes: &[Shape], route: Option<Route>, placement: Affine2) -> Scene {
    scene_with(
        shapes,
        route,
        placement,
        &zgui_paint::content::NoVectorMasks,
    )
}

/// `shapes` on black under `placement`, drawn through the path glyph route of a frame of
/// `content`, whose tiles reach `renderer`.
pub(crate) fn scene_of_glyphs(
    shapes: &[Shape],
    placement: Affine2,
    content: &mut ContentCache,
    renderer: &mut WgpuRenderer,
) -> Scene {
    with_glyphs(content, renderer, |glyphs| {
        scene_with(shapes, Some(Route::Glyphs), placement, glyphs)
    })
}

/// The transform `placement` stands for in `scene`.
pub(crate) fn space(scene: &mut Scene, placement: Affine2) -> SpatialId {
    if placement == Affine2::IDENTITY {
        return SpatialId::VIEWPORT;
    }
    let viewport = scene.spatial.viewport();
    let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
    scene.spatial.space_of(
        viewport,
        owner,
        OwnSpace::of(Some(placement.to_matrix4()), None, false),
    )
}

/// `shapes` on black under `placement`, drawn with `route` allowed, the glyph route through
/// `glyphs`.
fn scene_with(
    shapes: &[Shape],
    route: Option<Route>,
    placement: Affine2,
    glyphs: &dyn VectorMaskSource,
) -> Scene {
    let mut scene = super::scene();
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    let transform = space(&mut scene, placement);
    let paint = ShapePaint {
        fill: Color::WHITE,
        stroke: None,
        stroke_width: 1.0,
    };
    let placement = VectorPlacement {
        clip: ClipId::ROOT,
        transform,
        scale: 1.0,
    };
    match route {
        Some(Route::Analytic) => {
            draw_with_masks(
                &mut scene,
                VectorId(1),
                shapes,
                paint,
                &AnalyticOnly,
                placement,
            );
        }
        Some(Route::Marks) => {
            draw_with_masks(
                &mut scene,
                VectorId(1),
                shapes,
                paint,
                &MarksOnly,
                placement,
            );
        }
        Some(Route::Glyphs) => {
            draw_with_masks(&mut scene, VectorId(1), shapes, paint, glyphs, placement);
        }
        None => {
            draw(&mut scene, VectorId(1), shapes, paint, placement);
        }
    }
    scene.finish(&DamageSet::full());
    scene
}

/// How one picture of some shapes agrees with another.
#[derive(Debug)]
pub(crate) struct Agreement {
    /// Pixels whose neighbourhood the reference paints in one colour.
    pub(crate) interior: u32,
    /// Those of them that a shape covers.
    pub(crate) covered: u32,
    /// The largest channel difference at an interior pixel.
    pub(crate) interior_worst: u8,
    /// Every other pixel that either picture paints.
    pub(crate) edge: u32,
    /// The mean of the largest channel difference over the edge pixels.
    pub(crate) mean: f64,
    /// The largest channel difference at one edge pixel.
    pub(crate) worst: u8,
    /// Where that pixel is.
    pub(crate) worst_at: (i32, i32),
}

/// How `picture` agrees with `reference`.
pub(crate) fn agreement(picture: &Pixels, reference: &Pixels) -> Agreement {
    let black = [0u8, 0, 0];
    let rgb = |pixels: &Pixels, x: i32, y: i32| {
        let [red, green, blue, _] = pixels.rgba(x, y);
        [red, green, blue]
    };
    let mut found = Agreement {
        interior: 0,
        covered: 0,
        interior_worst: 0,
        edge: 0,
        mean: 0.0,
        worst: 0,
        worst_at: (0, 0),
    };
    let mut sum = 0u64;
    for y in 0..SIDE {
        for x in 0..SIDE {
            let want = rgb(reference, x, y);
            let got = rgb(picture, x, y);
            let difference = (0..3).map(|c| got[c].abs_diff(want[c])).max().unwrap_or(0);
            let uniform = (-1..=1).all(|dy| {
                (-1..=1).all(|dx| {
                    let (nx, ny) = ((x + dx).clamp(0, SIDE - 1), (y + dy).clamp(0, SIDE - 1));
                    rgb(reference, nx, ny) == want
                })
            });
            if uniform {
                found.interior += 1;
                found.covered += u32::from(want != black);
                found.interior_worst = found.interior_worst.max(difference);
            } else if got != black || want != black {
                found.edge += 1;
                sum += u64::from(difference);
                if difference > found.worst {
                    found.worst = difference;
                    found.worst_at = (x, y);
                }
            }
        }
    }
    found.mean = sum as f64 / f64::from(found.edge.max(1));
    found
}

/// The largest length one local unit becomes under `placement`.
fn largest_scale(placement: Affine2) -> f64 {
    let [a, b, c, d] = [placement.a, placement.b, placement.c, placement.d].map(f64::from);
    let p = a * a + b * b;
    let q = c * c + d * d;
    let r = a * c + b * d;
    ((p + q) / 2.0 + (((p - q) / 2.0).powi(2) + r * r).sqrt()).sqrt()
}

/// The same shapes with every curve, cap and join replaced by lines within a two-hundredth of a
/// device pixel under `placement`, and every stroke by the outline it covers, filled.
pub(crate) fn precise(shapes: &[Shape], placement: Affine2) -> Vec<Shape> {
    let tolerance = 0.005 / largest_scale(placement);
    let lines = |path: &BezPath| {
        let mut out = BezPath::new();
        kurbo::flatten(path.iter(), tolerance, |element| out.push(element));
        Arc::new(out)
    };
    let mut out = Vec::new();
    for shape in shapes {
        let clips: Vec<zgui_svg::Clip> = shape
            .clips
            .iter()
            .map(|clip| zgui_svg::Clip {
                path: lines(&clip.path),
                rule: clip.rule,
            })
            .collect();
        if let Some(fill) = &shape.fill {
            out.push(Shape {
                path: lines(&shape.path),
                fill: Some(fill.clone()),
                stroke: None,
                clips: clips.clone(),
            });
        }
        if let Some(stroke) = &shape.stroke {
            let covered = kurbo::stroke(
                shape.path.iter(),
                &stroke.style,
                &kurbo::StrokeOpts::default(),
                tolerance,
            );
            out.push(Shape {
                path: lines(&covered),
                fill: Some(Fill {
                    paint: stroke.paint.clone(),
                    rule: peniko::Fill::NonZero,
                }),
                stroke: None,
                clips,
            });
        }
    }
    out
}

/// Draws `shapes` through `route` and through the general route under `placement`, and checks the
/// two pictures agree.
///
/// Interior pixels agree to one level, the rounding of a translucent paint through the path
/// renderer's scratch. Edge pixels agree within [`MEAN`] on average and [`WORST`] at most. Where a
/// curve makes them differ by more, both pictures are measured against the true coverage, and the
/// recognised one has to be the closer of the two.
pub(crate) fn compare(name: &str, shapes: &[Shape], placement: Affine2, route: Route) {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let quick = match route {
        Route::Glyphs => {
            let mut content = ContentCache::new(zgui_atlas::AtlasLimits::default());
            scene_of_glyphs(shapes, placement, &mut content, &mut harness.renderer)
        }
        _ => scene_of(shapes, Some(route), placement),
    };
    let general = scene_of(shapes, None, placement);
    let exact = scene_of(&precise(shapes, placement), None, placement);
    judge(name, &mut harness.renderer, &quick, &general, &exact, route);
}

/// Checks that `quick`, drawn through `route`, agrees with `general` as [`compare`] says, with
/// `exact` as the true coverage.
pub(crate) fn judge(
    name: &str,
    renderer: &mut WgpuRenderer,
    quick: &Scene,
    general: &Scene,
    exact: &Scene,
    route: Route,
) {
    assert!(
        quick.primitives.vectors.is_empty(),
        "{name}: a shape left the {route:?} route"
    );
    assert_eq!(
        general.primitives.quads.len(),
        1,
        "{name}: the general picture holds quads besides its background"
    );
    match route {
        Route::Analytic => assert!(
            quick.primitives.quads.len() > 1,
            "{name}: the analytic picture drew no quad"
        ),
        Route::Marks | Route::Glyphs => {
            assert!(
                !quick.primitives.marks.is_empty(),
                "{name}: the marks picture drew no mark"
            );
            assert_eq!(
                quick.primitives.quads.len(),
                1,
                "{name}: the marks picture holds quads besides its background"
            );
        }
    }
    let by_route = present(renderer, quick);
    let by_paths = present(renderer, general);
    let by_lines = present(renderer, exact);
    let found = agreement(&by_route, &by_paths);
    let route_error = agreement(&by_route, &by_lines);
    let paths_error = agreement(&by_paths, &by_lines);
    println!("{name}: against the general route {found:?}");
    println!("{name}: {route:?} against the true coverage {route_error:?}");
    println!("{name}: general against the true coverage {paths_error:?}");
    assert!(
        found.interior >= 2_000,
        "{name}: only {} interior pixels were compared",
        found.interior
    );
    assert!(
        found.covered >= 100 && found.edge >= 100,
        "{name}: the shapes cover almost nothing: {found:?}"
    );
    assert!(
        found.interior_worst <= 1,
        "{name}: the two differ by {} inside a flat area",
        found.interior_worst
    );
    let close = found.mean <= MEAN && found.worst <= WORST;
    let closer = route_error.mean <= paths_error.mean && route_error.worst <= WORST;
    assert!(
        close || closer,
        "{name}: the edges differ by {:.2} on average and {} at most, and the {route:?} picture \
         is {:.2} from the true coverage where the general one is {:.2}",
        found.mean,
        found.worst,
        route_error.mean,
        paths_error.mean
    );
}
