//! A drawing rasterised whole on the CPU against the same drawing recorded as paths.
//!
//! The CPU layer stands in for the general route on a static page, so it has to draw the same
//! picture: the same ramps, the same clips, the same strokes, at a whole box, at a box a fraction of
//! a pixel off the grid, and under a scale. Both pictures are drawn on a grey ground and compared
//! wherever either paints.

mod support;

use std::collections::HashMap;

use zgui_color::Color;
use zgui_dom::{Document, NodeKind};
use zgui_geom::{Affine2, DevicePx, Point, Rect, Size};
use zgui_interned::ElementName;
use zgui_paint::content::vectors::{LayerAnswer, LayerPixels, LayerRequest};
use zgui_paint::content::{VectorMaskSource as _, VectorPlacement as Placement};
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement, layer_image};
use zgui_paint::{ContentCache, VectorCache, VectorSource};
use zgui_paint::{Tables, VectorId};
use zgui_space::{ClipId, OwnSpace, PropertyOwner, SpatialId};
use zgui_testkit_compositor::Pixels;
use zgui_vocab::{PropKey, PropValue, prop::drawing};

use support::conformance::{self, SIDE, framed, scene_at};
use support::gpu::{Gpu, Picture};

/// The ground both pictures are drawn on.
const GROUND: [u8; 3] = [40, 44, 52];

/// What `currentColor` resolves to.
fn inherited() -> Color {
    Color::srgb_u8(255, 140, 20, 255)
}

/// How one document is placed.
#[derive(Clone, Copy, Debug)]
struct Case {
    /// The content box, as left, top, width and height in the fragment's space.
    box_: (f32, f32, f32, f32),
    /// The box's own transform, a scale about the origin.
    scale: f32,
}

/// The whole surface, a box off the pixel grid, and half the surface under a scale of two.
const CASES: [Case; 3] = [
    Case {
        box_: (0.0, 0.0, 128.0, 128.0),
        scale: 1.0,
    },
    Case {
        box_: (0.25, 0.5, 120.0, 120.0),
        scale: 1.0,
    },
    Case {
        box_: (0.0, 0.0, 64.0, 64.0),
        scale: 2.0,
    },
];

/// The scene a case is drawn in, and the transform the drawing is placed under.
fn ground(case: Case) -> (Tables, SpatialId) {
    let mut scene = scene_at(SIDE as u32);
    let transform = if case.scale == 1.0 {
        SpatialId::VIEWPORT
    } else {
        let viewport = scene.spatial.viewport();
        let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
        let matrix = Affine2::new(case.scale, 0.0, 0.0, case.scale, 0.0, 0.0).to_matrix4();
        scene
            .spatial
            .space_of(viewport, owner, OwnSpace::of(Some(matrix), None, false))
    };
    (scene, transform)
}

/// Records the ground over the whole target.
fn paint_ground(rec: &mut zgui_paint::emit::Recorder<'_>) {
    let ground = rec.color(Color::srgb_u8(GROUND[0], GROUND[1], GROUND[2], 255));
    let mut quad = zgui_paint::emit::record::filled(
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(SIDE as f32), DevicePx(SIDE as f32)),
        ),
        [0.0; 8],
        ground,
    );
    quad.space = rec.space(SpatialId::VIEWPORT);
    rec.quad(quad);
}

/// The placement of a drawing under `transform`.
fn placed(transform: SpatialId) -> VectorPlacement {
    VectorPlacement {
        clip: ClipId::ROOT,
        transform,
        scale: 1.0,
    }
}

/// The drawing an element carrying `source` draws into the case's box.
fn drawing_of(source: &str, case: Case) -> (zgui_paint::content::Drawing, u64) {
    let mut document = Document::new();
    let index = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("vector"),
    );
    document
        .edit(&zgui_dom::EverythingMatters, |edit| {
            edit.set_property(
                index,
                PropKey::new(drawing::DOCUMENT),
                Some(PropValue::from(source)),
            );
        })
        .expect("not poisoned");
    let node = document.store().key_of(index);
    let (left, top, width, height) = case.box_;
    let cache = VectorCache::new();
    let frame = cache.frame(&document);
    let drawing = frame
        .drawing(
            node,
            Placement {
                content_box: Rect::new(
                    Point::new(DevicePx(left), DevicePx(top)),
                    Size::new(DevicePx(width), DevicePx(height)),
                ),
                scale: 1.0,
            },
        )
        .expect("the element draws a document");
    (drawing, frame.revision(node))
}

fn paint() -> ShapePaint {
    ShapePaint {
        fill: inherited(),
        stroke: None,
        stroke_width: 1.0,
    }
}

/// The drawing through the general route, kept on frame paths.
fn general(gpu: &mut Gpu, source: &str, case: Case) -> Picture {
    let (mut scene, transform) = ground(case);
    let (drawing, _) = drawing_of(source, case);
    let shapes = framed(&drawing.placed_all());
    let picture = gpu.picture(&mut scene, transform, |rec| {
        paint_ground(rec);
        zgui_paint::emit::vector::draw(rec, VectorId(1), &shapes, paint(), placed(transform));
    });
    assert!(
        picture.drawing.of_kind("path").count() > 0,
        "the general route drew nothing: {}",
        picture.drawing
    );
    picture
}

/// The drawing flattened to lines within a two-hundredth of a pixel, strokes replaced by their
/// outlines, through the general route: the true coverage.
fn exact(gpu: &mut Gpu, source: &str, case: Case) -> Picture {
    let (mut scene, transform) = ground(case);
    let (drawing, _) = drawing_of(source, case);
    let placement = Affine2::new(case.scale, 0.0, 0.0, case.scale, 0.0, 0.0);
    let shapes = framed(&conformance::precise(&drawing.placed_all(), placement));
    gpu.picture(&mut scene, transform, |rec| {
        paint_ground(rec);
        zgui_paint::emit::vector::draw(rec, VectorId(1), &shapes, paint(), placed(transform));
    })
}

/// The drawing as one CPU layer, rasterised by `content`.
fn layered(gpu: &mut Gpu, source: &str, case: Case, content: &mut ContentCache) -> Picture {
    let (mut scene, transform) = ground(case);
    let (drawing, revision) = drawing_of(source, case);
    let spatial = scene
        .spatial
        .resolve(transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2);
    content.begin_frame();
    let answer = content.layer(LayerRequest {
        owner: VectorId(1),
        revision,
        drawing: &drawing,
        paint: paint(),
        spatial,
    });
    let LayerAnswer::Sprite {
        raster,
        local,
        provisional: false,
    } = answer
    else {
        panic!("expected an exact layer, got {answer:?}");
    };
    gpu.picture(&mut scene, transform, |rec| {
        paint_ground(rec);
        layer_image(rec, local, &raster, placed(transform), 1.0);
    })
}

/// How two pictures differ wherever either paints: the mean and the largest difference of the
/// widest channel, and where the largest is.
fn difference(one: &Pixels, two: &Pixels) -> (f64, u8, (i32, i32), u32) {
    let rgb = |pixels: &Pixels, x: i32, y: i32| {
        let [red, green, blue, _] = pixels.rgba(x, y);
        [red, green, blue]
    };
    let (mut sum, mut worst, mut at, mut count) = (0_u64, 0_u8, (0, 0), 0_u32);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let (a, b) = (rgb(one, x, y), rgb(two, x, y));
            if a == GROUND && b == GROUND {
                continue;
            }
            let apart = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
            sum += u64::from(apart);
            count += 1;
            if apart > worst {
                worst = apart;
                at = (x, y);
            }
        }
    }
    (sum as f64 / f64::from(count.max(1)), worst, at, count)
}

/// The documents of the path renderer's own tests, the static bench's and the gallery's.
fn conformance_set() -> Vec<(&'static str, String)> {
    let clip_rule = |rule: &str| {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><defs><clipPath id="c"><path clip-rule="{rule}" d="M8 8 H120 V120 H8 Z M48 48 H80 V80 H48 Z"/></clipPath></defs><g clip-path="url(#c)"><rect x="0" y="0" width="128" height="128" fill="#20a040"/></g></svg>"##
        )
    };
    let mut set: Vec<(&'static str, String)> = vec![
        ("two colours", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><rect x="0" y="0" width="64" height="128" fill="#e01020"/><rect x="64" y="0" width="64" height="128" fill="#1030d0"/></svg>"##.into()),
        ("current colour", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><rect x="0" y="0" width="128" height="128" fill="currentColor"/></svg>"##.into()),
        ("ramp", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><defs><linearGradient id="g" x1="0" y1="0" x2="128" y2="0" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs><rect x="0" y="0" width="128" height="128" fill="url(#g)"/></svg>"##.into()),
        ("reflected", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><defs><linearGradient id="g" x1="0" y1="0" x2="32" y2="0" spreadMethod="reflect" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#00ff00"/></linearGradient></defs><rect x="0" y="0" width="128" height="128" fill="url(#g)"/></svg>"##.into()),
        ("clipped", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><defs><clipPath id="c"><rect x="32" y="32" width="64" height="64"/></clipPath></defs><g clip-path="url(#c)"><rect x="0" y="0" width="128" height="128" fill="#20a040"/></g></svg>"##.into()),
        ("clip rule nonzero", clip_rule("nonzero")),
        ("clip rule evenodd", clip_rule("evenodd")),
        ("dashed", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><path d="M0 64 H128" stroke="#f0f0f0" stroke-width="24" stroke-dasharray="16 16"/></svg>"##.into()),
        ("wide", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10"><rect x="0" y="0" width="20" height="10" fill="#f0f0f0"/></svg>"##.into()),
        ("both kinds", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><rect x="0" y="0" width="42" height="128" fill="#e01020"/><rect x="42" y="0" width="44" height="128" fill="currentColor"/><rect x="86" y="0" width="42" height="128" fill="#1030d0"/></svg>"##.into()),
        ("facet", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><defs><linearGradient id="ramp" x1="0" y1="0" x2="64" y2="64" gradientUnits="userSpaceOnUse"><stop offset="0.25" stop-color="#f43f5e"/><stop offset="0.5" stop-color="#f59e0b"/><stop offset="0.75" stop-color="#2563eb"/></linearGradient><clipPath id="facet"><path d="M32 1 L63 32 L32 63 L1 32 Z"/></clipPath></defs><g clip-path="url(#facet)"><rect x="0" y="0" width="64" height="64" fill="url(#ramp)"/></g></svg>"##.into()),
        ("star", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10.6" fill="none" stroke="currentColor" stroke-width="1.4"/><path fill="currentColor" d="M12 4 L14 9.4 L19.6 9.7 L15.3 13.3 L16.7 18.8 L12 15.7 L7.3 18.8 L8.7 13.3 L4.4 9.7 L10 9.4 Z"/></svg>"##.into()),
        ("banner", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 16"><rect x="1" y="1" width="46" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"/><path fill="currentColor" d="M5 12.5 L11 4 L17 12.5 Z"/><circle cx="24" cy="8" r="4" fill="currentColor"/><rect x="35" y="4" width="8" height="8" fill="currentColor"/></svg>"##.into()),
    ];
    for (index, source) in STATIC.iter().enumerate() {
        let name = ["static 1", "static 2", "static 3", "static 4"][index];
        set.push((name, (*source).into()));
    }
    set
}

/// The four documents of the static SVG bench.
const STATIC: [&str; 4] = [
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#4f8cff"/><stop offset="1" stop-color="#1b2a5c"/></linearGradient><radialGradient id="b" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="#ffd166"/><stop offset="1" stop-color="#ef476f"/></radialGradient><clipPath id="c"><circle cx="48" cy="48" r="30"/></clipPath></defs><rect x="4" y="4" width="88" height="88" rx="12" fill="url(#a)"/><circle cx="48" cy="48" r="26" fill="url(#b)"/><g clip-path="url(#c)"><rect x="20" y="40" width="56" height="16" fill="#06d6a0"/></g><path d="M14 80 L30 64 L46 80 Z" fill="#ffffff"/><path d="M60 18 L82 18 L82 26 L60 26 Z" fill="#118ab2"/></svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="0" y1="1" x2="0" y2="0"><stop offset="0" stop-color="#2a9d8f"/><stop offset="1" stop-color="#e9c46a"/></linearGradient><radialGradient id="b" cx="0.3" cy="0.3" r="0.7"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#264653"/></radialGradient><clipPath id="c"><rect x="16" y="16" width="64" height="40"/></clipPath></defs><circle cx="48" cy="48" r="44" fill="url(#b)"/><path d="M8 88 L48 20 L88 88 Z" fill="url(#a)"/><g clip-path="url(#c)"><circle cx="48" cy="56" r="28" fill="#e76f51"/></g><rect x="40" y="70" width="16" height="16" fill="#f4a261"/><path d="M10 10 L22 10 L22 22 Z" fill="#8ecae6"/></svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#8338ec"/><stop offset="1" stop-color="#3a86ff"/></linearGradient><radialGradient id="b" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="#fb5607"/><stop offset="1" stop-color="#ffbe0b"/></radialGradient><clipPath id="c"><path d="M48 8 L88 48 L48 88 L8 48 Z"/></clipPath></defs><rect x="0" y="0" width="96" height="96" fill="#1d1d2c"/><g clip-path="url(#c)"><rect x="0" y="0" width="96" height="96" fill="url(#a)"/></g><circle cx="48" cy="48" r="18" fill="url(#b)"/><path d="M20 20 L32 20 L32 32 L20 32 Z" fill="#ff006e"/><path d="M64 64 L76 64 L76 76 L64 76 Z" fill="#ffffff"/></svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="1" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ef233c"/><stop offset="1" stop-color="#2b2d42"/></linearGradient><radialGradient id="b" cx="0.5" cy="0.6" r="0.5"><stop offset="0" stop-color="#edf2f4"/><stop offset="1" stop-color="#8d99ae"/></radialGradient><clipPath id="c"><circle cx="48" cy="40" r="24"/></clipPath></defs><rect x="8" y="8" width="80" height="80" rx="40" fill="url(#b)"/><g clip-path="url(#c)"><path d="M0 40 L96 40 L96 96 L0 96 Z" fill="url(#a)"/></g><path d="M30 70 L66 70 L66 78 L30 78 Z" fill="#d90429"/><circle cx="20" cy="20" r="6" fill="#2b2d42"/><circle cx="76" cy="20" r="6" fill="#06d6a0"/></svg>"##,
];

#[test]
fn a_layer_matches_the_general_route_on_the_conformance_set() {
    let Some(mut gpu) = Gpu::open(SIDE as u32) else {
        return;
    };
    let mut failures = Vec::new();
    for (name, source) in conformance_set() {
        for case in CASES {
            let mut content = ContentCache::new();
            let by_layer = layered(&mut gpu, &source, case, &mut content).pixels;
            let by_paths = general(&mut gpu, &source, case).pixels;
            let (mean, worst, at, count) = difference(&by_layer, &by_paths);
            println!(
                "{name} {case:?}: mean {mean:.3}, worst {worst} at {at:?} over {count} pixels"
            );
            assert!(count > 100, "{name}: the drawing covers almost nothing");
            let close = mean <= conformance::MEAN && worst <= conformance::WORST;
            // Where a curve makes the two differ by more, both are measured against the true
            // coverage, and the layer has to be the closer of the two.
            let closer = close || {
                let by_lines = exact(&mut gpu, &source, case).pixels;
                let layer_error = difference(&by_layer, &by_lines);
                let paths_error = difference(&by_paths, &by_lines);
                println!(
                    "    against the true coverage: layer {:.3} worst {}, general {:.3} worst {}",
                    layer_error.0, layer_error.1, paths_error.0, paths_error.1
                );
                layer_error.0 <= paths_error.0 && layer_error.1 <= conformance::WORST
            };
            if !closer {
                failures.push(format!(
                    "{name} {case:?}: mean {mean:.3}, worst {worst} at {at:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A 3000 by 3000 document: a ramp over a quadrilateral, a radial disc, a clipped band and a stroke,
/// each across the tile corner at (1024, 1024).
const HUGE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 3000 3000"><defs><linearGradient id="a" x1="900" y1="900" x2="1150" y2="1150" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#203040"/><stop offset="1" stop-color="#a0c0e0"/></linearGradient><radialGradient id="b" cx="1024" cy="1024" r="40" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#ffd166"/><stop offset="1" stop-color="#ef476f"/></radialGradient><clipPath id="c"><circle cx="1060" cy="1000" r="30"/></clipPath></defs><path d="M0 0 L3000 100 L2900 3000 L50 2950 Z" fill="url(#a)"/><circle cx="1024" cy="1024" r="40" fill="url(#b)"/><g clip-path="url(#c)"><rect x="1000" y="990" width="120" height="20" fill="#06d6a0"/></g><path d="M990 1110 L1110 1010" fill="none" stroke="#ffffff" stroke-width="5"/></svg>"##;

#[test]
fn tiles_of_a_huge_drawing_match_the_general_route() {
    let Some(mut gpu) = Gpu::open(SIDE as u32) else {
        return;
    };
    // The surface shows document units 1000 to 1128, on the whole grid and off it.
    for (left, top) in [(-1000.0, -1000.0), (-1000.25, -999.5)] {
        let case = Case {
            box_: (left, top, 3000.0, 3000.0),
            scale: 1.0,
        };
        let (mut scene, transform) = ground(case);
        let (drawing, revision) = drawing_of(HUGE, case);
        let mut content = ContentCache::new();
        content.begin_frame();
        let answer = content.layer(LayerRequest {
            owner: VectorId(1),
            revision,
            drawing: &drawing,
            paint: paint(),
            spatial: Some(Affine2::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)),
        });
        let LayerAnswer::Tiles { tiles, .. } = answer else {
            panic!("expected tiles, got {answer:?}");
        };
        let placed_tiles: Vec<_> = tiles
            .tiles()
            .iter()
            .map(|tile| {
                let local = drawing.fit.transform_rect_bbox(tile.path_rect);
                let local = Rect::new(
                    Point::new(DevicePx(local.x0 as f32), DevicePx(local.y0 as f32)),
                    Size::new(
                        DevicePx(local.width() as f32),
                        DevicePx(local.height() as f32),
                    ),
                );
                (tile.pixels.clone(), local)
            })
            .collect();
        let mut rasters = HashMap::new();
        {
            let store = zgui_layout::tree::store::LayoutStore::new(zgui_arena::DocumentId::FIRST);
            let frame = content.frame(&store, &NoShaped);
            let layers = frame.layers().expect("a content frame keeps layers");
            for (pixels, local) in &placed_tiles {
                layers.shown(pixels.key, Some(*local));
            }
            let owed = layers.settle(
                Size::new(SIDE, SIDE),
                &zgui_bits::DamageSet::full(),
                &mut |raster: &LayerPixels| {
                    rasters.insert(raster.key, raster.clone());
                },
            );
            assert!(owed.is_empty(), "four tiles fit one frame's budget");
        }
        let by_tiles = gpu
            .picture(&mut scene, transform, |rec| {
                paint_ground(rec);
                for (pixels, local) in &placed_tiles {
                    let raster = rasters.get(&pixels.key).unwrap_or(pixels);
                    layer_image(rec, *local, raster, placed(transform), 1.0);
                }
            })
            .pixels;
        let by_paths = general(&mut gpu, HUGE, case).pixels;
        let (mean, worst, at, count) = difference(&by_tiles, &by_paths);
        println!("{left}, {top}: mean {mean:.3}, worst {worst} at {at:?} over {count} pixels");
        assert!(count > 10_000, "the drawing covers the surface");
        assert!(mean <= conformance::MEAN, "mean {mean:.3}");
        // Off the pixel grid the images are filtered, and still within the conformance worst.
        assert!(worst <= conformance::WORST, "worst {worst} at {at:?}");
    }
}

/// A source of positioned glyphs with nothing in it.
struct NoShaped;

impl zgui_text::ShapedGlyphs for NoShaped {
    fn visit_line(
        &self,
        _paragraph: zgui_text::ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
    ) {
    }

    fn brush(&self, _brush: zgui_text::Brush) -> Option<zgui_text::TextPaint> {
        None
    }
}
