//! One list of shapes recorded through a recognising route and through the general route, drawn
//! by the compositor on a device, and both measured against the true coverage.
//!
//! Where the path renderer paints one flat colour the two pictures have to be equal. Along an edge
//! they may differ by the antialiasing rule: the analytic shaders ramp coverage over one pixel of
//! distance, and the path renderer measures area. A third picture stands for the true coverage:
//! the shapes flattened to lines within a two-hundredth of a pixel, strokes replaced by their
//! outlines, drawn by the path renderer, which is exact on straight edges.

use std::cell::{RefCell, RefMut};
use std::sync::Arc;

use kurbo::{self, BezPath};
use zgui_color::Color;
use zgui_geom::Affine2;
use zgui_paint::content::vectors::{MarkPayloads, Splits};
use zgui_paint::content::{AnalyticOnly, MarksOnly, NoVectorMasks, VectorMaskSource};
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement, draw_with_masks};
use zgui_paint::{Tables, VectorId};
use zgui_space::{ClipId, OwnSpace, PropertyOwner, SpatialId};
use zgui_svg::{Fill, Shape};
use zgui_testkit_compositor::{Drawing, Pixels};

use super::gpu::{Gpu, Picture};

/// The side of the square target every picture is drawn on.
pub(crate) const SIDE: i32 = 128;

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

/// A source that allows the path glyph route alone, and keeps path splits and mark payloads.
#[derive(Debug, Default)]
pub(crate) struct GlyphsOnly {
    splits: RefCell<Splits>,
    payloads: RefCell<MarkPayloads>,
}

impl VectorMaskSource for GlyphsOnly {
    fn analytic(&self, _owner: VectorId) -> bool {
        false
    }

    fn marks(&self, _owner: VectorId) -> bool {
        false
    }

    fn glyph_splits(&self) -> Option<RefMut<'_, Splits>> {
        Some(self.splits.borrow_mut())
    }

    fn payloads(&self) -> Option<RefMut<'_, MarkPayloads>> {
        Some(self.payloads.borrow_mut())
    }
}

/// The route source of `route`, or of the general route alone.
pub(crate) fn source(route: Option<Route>) -> Box<dyn VectorMaskSource> {
    match route {
        Some(Route::Analytic) => Box::new(AnalyticOnly),
        Some(Route::Marks) => Box::new(MarksOnly),
        Some(Route::Glyphs) => Box::new(GlyphsOnly::default()),
        None => Box::new(NoVectorMasks),
    }
}

/// The transform `placement` stands for in `scene`.
pub(crate) fn space(scene: &mut Tables, placement: Affine2) -> SpatialId {
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

/// A scene over a square target of `side` pixels.
pub(crate) fn scene_at(side: u32) -> Tables {
    let mut scene = Tables::new();
    scene.begin_frame(zgui_geom::Size::new(side as i32, side as i32));
    scene
}

/// `shapes` in white on black under `placement`, recorded with `route` allowed, or through the
/// general route alone.
pub(crate) fn picture(
    gpu: &mut Gpu,
    shapes: &[Shape],
    route: Option<Route>,
    placement: Affine2,
) -> Picture {
    let routes = source(route);
    let mut scene = scene_at(gpu.side());
    let transform = space(&mut scene, placement);
    gpu.picture(&mut scene, transform, |rec| {
        draw_with_masks(
            rec,
            VectorId(1),
            shapes,
            PAINT,
            routes.as_ref(),
            VectorPlacement {
                clip: ClipId::ROOT,
                transform,
                scale: 1.0,
            },
        );
    })
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

/// The same shapes clipped by a path that admits everything, which keeps them frame paths.
///
/// A compositor may lower an unclipped path to path masks, and never lowers one with a path clip
/// (spec "PathItem").
pub(crate) fn framed(shapes: &[Shape]) -> Vec<Shape> {
    use kurbo::Shape as _;

    let everything = Arc::new(kurbo::Rect::new(-4096.0, -4096.0, 4096.0, 4096.0).to_path(0.1));
    shapes
        .iter()
        .cloned()
        .map(|mut shape| {
            shape.clips.push(zgui_svg::Clip {
                path: Arc::clone(&everything),
                rule: peniko::Fill::NonZero,
            });
            shape
        })
        .collect()
}

/// Records `shapes` through `route` and through the general route under `placement`, and checks
/// the two pictures agree.
///
/// Interior pixels agree to one level, the rounding of a translucent paint through the path
/// renderer. Edge pixels agree within [`MEAN`] on average and [`WORST`] at most. Where a curve
/// makes them differ by more, both pictures are measured against the true coverage, and the
/// recognised one has to be the closer of the two.
pub(crate) fn compare(name: &str, shapes: &[Shape], placement: Affine2, route: Route) {
    let Some(mut gpu) = Gpu::open(SIDE as u32) else {
        return;
    };
    let quick = picture(&mut gpu, shapes, Some(route), placement);
    let general = picture(&mut gpu, &framed(shapes), None, placement);
    let exact = picture(
        &mut gpu,
        &framed(&precise(shapes, placement)),
        None,
        placement,
    );
    judge(name, &quick, &general, &exact, route);
}

/// Checks that `quick`, recorded through `route`, agrees with `general` as [`compare`] says, with
/// `exact` as the true coverage.
pub(crate) fn judge(name: &str, quick: &Picture, general: &Picture, exact: &Picture, route: Route) {
    shaped(name, &quick.drawing, &general.drawing, route);
    let found = agreement(&quick.pixels, &general.pixels);
    let route_error = agreement(&quick.pixels, &exact.pixels);
    let paths_error = agreement(&general.pixels, &exact.pixels);
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

/// The instances a path item draws as: frame paths, or mono masks the compositor lowered it to.
fn paths(drawing: &Drawing) -> usize {
    drawing.of_kind("path").count() + drawing.of_kind("mono").count()
}

/// Checks that `quick` drew `route`'s items alone and `general` drew paths over the ground.
fn shaped(name: &str, quick: &Drawing, general: &Drawing, route: Route) {
    assert_eq!(
        paths(quick),
        0,
        "{name}: a shape left the {route:?} route: {quick}"
    );
    assert_eq!(
        general.quads().len(),
        1,
        "{name}: the general picture holds quads besides its ground: {general}"
    );
    assert!(paths(general) > 0, "{general}");
    match route {
        Route::Analytic => assert!(
            quick.quads().len() > 1,
            "{name}: the analytic picture drew no quad: {quick}"
        ),
        Route::Marks | Route::Glyphs => {
            assert!(
                quick.of_kind("marks").count() > 0,
                "{name}: the marks picture drew no mark: {quick}"
            );
            assert_eq!(
                quick.quads().len(),
                1,
                "{name}: the marks picture holds quads besides its ground: {quick}"
            );
        }
    }
}

/// An opaque colour from three bytes.
pub(crate) fn opaque(red: u8, green: u8, blue: u8) -> Color {
    Color::srgb_u8(red, green, blue, 255)
}

/// `drawing` in white on black at the origin, recorded through `routes`.
pub(crate) fn drawn(
    gpu: &mut Gpu,
    drawing: &zgui_paint::content::Drawing,
    routes: &dyn VectorMaskSource,
) -> Picture {
    let mut scene = scene_at(gpu.side());
    gpu.picture(&mut scene, SpatialId::VIEWPORT, |rec| {
        zgui_paint::emit::vector::draw_drawing(rec, VectorId(1), drawing, PAINT, routes, PLACEMENT);
    })
}

/// What `drawing` records at the origin through `routes`.
pub(crate) fn recording(
    drawing: &zgui_paint::content::Drawing,
    routes: &dyn VectorMaskSource,
) -> zgui_paint::emit::record::Recording {
    let mut scene = scene_at(SIDE as u32);
    let mut recording = zgui_paint::emit::record::Recording::new();
    let mut rec = recording.recorder(&mut scene, SpatialId::VIEWPORT, ClipId::ROOT);
    zgui_paint::emit::vector::draw_drawing(
        &mut rec,
        VectorId(1),
        drawing,
        PAINT,
        routes,
        PLACEMENT,
    );
    recording
}

/// The payloads of the marks items `drawing` records through `routes`, in record order.
pub(crate) fn payloads(
    drawing: &zgui_paint::content::Drawing,
    routes: &dyn VectorMaskSource,
) -> Vec<Arc<compositor_types::MarkPayload>> {
    let recording = recording(drawing, routes);
    recording
        .marks()
        .iter()
        .map(|marks| Arc::clone(recording.payload(marks)))
        .collect()
}

/// Reports whether `a` and `b` are the same payloads, by address.
pub(crate) fn same_payloads(
    a: &[Arc<compositor_types::MarkPayload>],
    b: &[Arc<compositor_types::MarkPayload>],
) -> bool {
    !a.is_empty() && a.len() == b.len() && a.iter().zip(b).all(|(a, b)| Arc::ptr_eq(a, b))
}

/// White fill, the inherited colour of every shape here.
pub(crate) const PAINT: ShapePaint = ShapePaint {
    fill: Color::WHITE,
    stroke: None,
    stroke_width: 1.0,
};

/// Drawn at the origin of the target, with no transform and no clip.
pub(crate) const PLACEMENT: VectorPlacement = VectorPlacement {
    clip: ClipId::ROOT,
    transform: SpatialId::VIEWPORT,
    scale: 1.0,
};
