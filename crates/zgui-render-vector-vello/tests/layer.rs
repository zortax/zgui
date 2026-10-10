//! A drawing rasterised whole on the CPU against the same drawing through the path renderer.
//!
//! The CPU layer stands in for the general route on a static page, so it has to draw the same
//! picture: the same ramps, the same clips, the same strokes, at a whole box, at a box a fraction of
//! a pixel off the grid, and under a scale. Both pictures are drawn on a grey ground and compared
//! wherever either paints.

mod support;

use zgui_atlas::AtlasLimits;
use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_dom::{Document, NodeKind};
use zgui_geom::{Affine2, DevicePx, Point, Rect, Size};
use zgui_interned::ElementName;
use zgui_paint::content::vectors::{LayerAnswer, LayerRequest};
use zgui_paint::content::{NoVectorMasks, VectorPlacement as Placement};
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement, draw_drawing, layer_sprite};
use zgui_paint::{ContentCache, VectorCache, VectorSource};
use zgui_render::Renderer as _;
use zgui_render_wgpu::Pixels;
use zgui_scene::{ClipId, OwnSpace, PropertyOwner, Scene, SpatialId, VectorId};
use zgui_vocab::{PropKey, PropValue, prop::drawing};

use support::{SIDE, Which, harness, opaque, present, quad, rect};

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

/// The scene a case is drawn in: the ground, and the transform the drawing is placed under.
fn ground(case: Case) -> (Scene, SpatialId) {
    let mut scene = support::scene();
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(GROUND[0], GROUND[1], GROUND[2]),
    );
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

/// The drawing through the general route.
fn general(source: &str, case: Case) -> Scene {
    let (mut scene, transform) = ground(case);
    let (drawing, _) = drawing_of(source, case);
    draw_drawing(
        &mut scene,
        VectorId(1),
        &drawing,
        paint(),
        &NoVectorMasks,
        VectorPlacement {
            clip: ClipId::ROOT,
            transform,
            scale: 1.0,
        },
    );
    assert!(
        !scene.primitives.vectors.is_empty(),
        "the general route drew nothing"
    );
    scene.finish(&DamageSet::full());
    scene
}

/// The drawing flattened to lines within a two-hundredth of a pixel, strokes replaced by their
/// outlines, through the general route: the true coverage.
fn exact(source: &str, case: Case) -> Scene {
    let (mut scene, transform) = ground(case);
    let (drawing, _) = drawing_of(source, case);
    let placement = Affine2::new(case.scale, 0.0, 0.0, case.scale, 0.0, 0.0);
    let shapes = support::conformance::precise(&drawing.placed_all(), placement);
    zgui_paint::emit::vector::draw(
        &mut scene,
        VectorId(1),
        &shapes,
        paint(),
        VectorPlacement {
            clip: ClipId::ROOT,
            transform,
            scale: 1.0,
        },
    );
    scene.finish(&DamageSet::full());
    scene
}

/// The drawing as one CPU layer, with its tile in `content`.
fn layered(source: &str, case: Case, content: &mut ContentCache) -> Scene {
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
        tile,
        local,
        provisional: false,
        ..
    } = answer
    else {
        panic!("expected an exact layer, got {answer:?}");
    };
    let sprite = layer_sprite(
        local,
        tile,
        VectorPlacement {
            clip: ClipId::ROOT,
            transform,
            scale: 1.0,
        },
        1.0,
    );
    scene.push_color_sprite(sprite);
    scene.finish(&DamageSet::full());
    scene
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
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut failures = Vec::new();
    for (name, source) in conformance_set() {
        for case in CASES {
            let mut content = ContentCache::new(AtlasLimits::default());
            let layer = layered(&source, case, &mut content);
            content
                .flush(harness.renderer.texture_sink())
                .expect("the layer's texels reach the device");
            let by_layer = present(&mut harness.renderer, &layer);
            let by_paths = present(&mut harness.renderer, &general(&source, case));
            let (mean, worst, at, count) = difference(&by_layer, &by_paths);
            println!(
                "{name} {case:?}: mean {mean:.3}, worst {worst} at {at:?} over {count} pixels"
            );
            assert!(count > 100, "{name}: the drawing covers almost nothing");
            let close = mean <= support::conformance::MEAN && worst <= support::conformance::WORST;
            // Where a curve makes the two differ by more, both are measured against the true
            // coverage, and the layer has to be the closer of the two.
            let closer = close || {
                let by_lines = present(&mut harness.renderer, &exact(&source, case));
                let layer_error = difference(&by_layer, &by_lines);
                let paths_error = difference(&by_paths, &by_lines);
                println!(
                    "    against the true coverage: layer {:.3} worst {}, general {:.3} worst {}",
                    layer_error.0, layer_error.1, paths_error.0, paths_error.1
                );
                layer_error.0 <= paths_error.0 && layer_error.1 <= support::conformance::WORST
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
