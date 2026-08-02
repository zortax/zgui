//! The seam: two rasterisers, one contract, one scene.
//!
//! This is what makes the rasteriser a seam rather than one implementation wearing a trait's
//! clothes. The same display list goes through both, on the same device, and the two have to agree
//! about *what is covered* — not about every last edge pixel, because one decides an edge by
//! analytic area and the other by counting sixteen samples, and those cannot agree exactly.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{DevicePx, Vec2};
use zgui_scene::{ClipId, ClipLink, Scene};

use support::{Which, both, circle, harness, opaque, path, present, quad, rect, scene, vector};

/// The scene both rasterisers are given.
fn shared_scene() -> Scene {
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    // A rectangle, a circle, and a rounded-clipped rectangle: a straight edge, a curved one, and a
    // residual clip the rasteriser has to apply inside its own scratch.
    vector(
        &mut scene,
        0,
        path(rect(8.0, 8.0, 40.0, 40.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    vector(
        &mut scene,
        1,
        circle(90.0, 28.0, 18.0),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    let clip = scene.clips.only(ClipLink::rounded(
        rect(24.0, 72.0, 80.0, 40.0),
        Vec2::new(DevicePx(20.0), DevicePx(10.0)),
    ));
    vector(
        &mut scene,
        2,
        path(rect(16.0, 64.0, 96.0, 56.0)),
        opaque(255, 255, 255),
        clip,
    );
    scene.finish(&DamageSet::full());
    scene
}

/// How far the two disagree, and over how many pixels.
fn compare(left: &zgui_render_wgpu::Pixels, right: &zgui_render_wgpu::Pixels) -> (u8, u32, u32) {
    let mut worst = 0u8;
    let mut differing = 0u32;
    let mut interior = 0u32;
    for y in 0..left.size().height {
        for x in 0..left.size().width {
            let (a, b) = (left.rgba(x, y), right.rgba(x, y));
            let error = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
            worst = worst.max(error);
            if error > 0 {
                differing += 1;
            }
            // A pixel both agree is wholly covered, or wholly uncovered, is an interior pixel.
            if (a[0] == 255 && b[0] == 255) || (a[0] == 0 && b[0] == 0) {
                interior += 1;
                assert_eq!(
                    a, b,
                    "the two disagree at ({x}, {y}), which is not on an edge"
                );
            }
        }
    }
    (worst, differing, interior)
}

/// The same scene through both rasterisers covers the same pixels.
#[test]
fn both_rasterisers_cover_the_same_fragments_within_the_documented_tolerance() {
    let Some((mut compute, mut coverage)) = both() else {
        return;
    };
    let scene = shared_scene();
    let by_compute = present(&mut compute.renderer, &scene);
    let by_coverage = present(&mut coverage, &scene);

    let (worst, differing, interior) = compare(&by_compute, &by_coverage);
    println!(
        "vello vs coverage: worst {worst}, differing {differing}, interior agreeing {interior}"
    );
    assert!(
        interior > 12_000,
        "only {interior} pixels are interior, so almost nothing was actually compared"
    );
    // **The documented tolerance, and where it comes from.** Two things separate the two answers,
    // and only the smaller one is the coverage rule: sixteen samples resolve an edge to a
    // seventeenth, which is 16 of 255. The larger one is *curve flattening*. On a circle of radius
    // eighteen the compute rasteriser loses about a sixth of the area of the one-pixel cap at the
    // top, where the sampled one loses about a fiftieth — measured, on this device, at up to 61 of
    // 255 on a single edge pixel. Straight edges agree to one level, which is what pins the cause
    // on the curve rather than on the coverage rule.
    assert!(
        worst <= 72,
        "the two rasterisers disagree by {worst}, which is more than flattening and a sixteen-sample \
         grid account for between them"
    );
    assert!(
        differing < 500,
        "{differing} pixels differ, which is more than the edges of these three shapes"
    );
    assert!(
        differing > 0,
        "an assertion that the two agree exactly would be asserting something false"
    );
}

/// A straight edge is where the two have nothing to disagree about, which is what makes the
/// tolerance above a statement about curves rather than a blanket allowance.
#[test]
fn a_straight_edge_lands_within_one_level_in_both_rasterisers() {
    let Some((mut compute, mut coverage)) = both() else {
        return;
    };
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    // A quarter of a pixel of the left column and half of the top row are outside the shape, so
    // both rasterisers have a fraction to get right and neither has a curve to flatten.
    vector(
        &mut scene,
        0,
        path(rect(40.25, 40.5, 32.0, 32.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    let by_compute = present(&mut compute.renderer, &scene);
    let by_coverage = present(&mut coverage, &scene);

    for (at, expected) in [((40, 41), 191u8), ((41, 40), 128), ((40, 40), 96)] {
        let one = by_compute.rgba(at.0, at.1)[0];
        let other = by_coverage.rgba(at.0, at.1)[0];
        assert!(
            one.abs_diff(expected) <= 1 && other.abs_diff(expected) <= 1,
            "at {at:?} the two read {one} and {other}, and the fraction outside says {expected}"
        );
    }
}

/// The fallback honours a residual clip too, which is the part of the contract that is easiest to
/// quietly not implement.
#[test]
fn the_fallback_applies_a_residual_clip_inside_its_own_scratch() {
    let Some(mut harness) = harness(Which::Coverage) else {
        return;
    };
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    // A second item with a shallower chain drops the shared prefix, so the rounded link becomes a
    // residual the rasteriser has to apply itself.
    vector(
        &mut scene,
        1,
        path(rect(0.0, 0.0, 1.0, 1.0)),
        zgui_color::Color::srgb_u8(0, 0, 0, 0),
        ClipId::ROOT,
    );
    let clip = scene.clips.only(ClipLink::rounded(
        rect(32.0, 32.0, 64.0, 64.0),
        Vec2::splat(DevicePx(32.0)),
    ));
    vector(
        &mut scene,
        0,
        path(rect(0.0, 0.0, 128.0, 128.0)),
        opaque(255, 255, 255),
        clip,
    );
    scene.finish(&DamageSet::full());
    assert_eq!(scene.pass_plan().clip_layers, 1);

    let pixels = present(&mut harness.renderer, &scene);
    assert_eq!(
        pixels.rgba(64, 64),
        [255, 255, 255, 255],
        "the middle of the clip is inside it"
    );
    assert_eq!(
        pixels.rgba(34, 34),
        [0, 0, 0, 255],
        "the corner is outside a circle of radius 32, so the residual clip cut it away"
    );
    assert_eq!(
        pixels.rgba(8, 8),
        [0, 0, 0, 255],
        "and everything outside the clip's own box is untouched"
    );
}

/// Neither rasteriser draws an item whose residual clip it has no shape for.
///
/// What survives of a sampled mask in a display list is a coverage tile, not the shape it came
/// from — even when the shape *began* as a path, which is the case the coalescing policy keeps in
/// the pass rather than giving a pass of its own. Drawing the item anyway would be a clip that
/// silently did not apply, so it is left out and counted instead.
///
/// Driven against the rasterisers directly rather than through a frame, because a chain carrying a
/// sampled mask has no composite to bind either and the renderer refuses it first.
#[test]
fn an_item_whose_residual_clip_has_no_shape_is_left_out_and_counted() {
    let Some(harness) = harness(Which::Vello) else {
        return;
    };
    let gpu = std::sync::Arc::clone(harness.gpu());
    let mut scene = scene();
    // Two items: one under a mask chain, one under nothing, so the shared prefix is nothing and the
    // mask link becomes the first item's residual.
    let tile = zgui_atlas::AtlasTile {
        texture: zgui_atlas::TextureId::new(zgui_atlas::TextureKind::Mono, 0),
        tile: zgui_atlas::TileId(0),
        bounds: zgui_geom::Rect::new(zgui_geom::Point::new(0, 0), zgui_geom::Size::new(64, 64)),
    };
    let clip = scene.clips.only(ClipLink::Mask {
        tile,
        transform: zgui_scene::SpatialId::VIEWPORT,
        source: zgui_scene::MaskSource::Path,
    });
    vector(
        &mut scene,
        0,
        path(rect(16.0, 16.0, 64.0, 64.0)),
        opaque(255, 255, 255),
        clip,
    );
    vector(
        &mut scene,
        1,
        path(rect(96.0, 96.0, 16.0, 16.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    assert_eq!(
        scene.pass_plan().len(),
        1,
        "a path-sourced mask does not end the pass, so it reaches a rasteriser as a residual"
    );
    assert_eq!(scene.pass_plan().items.len(), 2);

    let mut compute =
        zgui_render_vector_vello::VelloRaster::new(&gpu, 128, 128).expect("a rasteriser");
    let plan = zgui_render::VectorRaster::plan(&mut compute, scene.pass_plan());
    zgui_render::VectorRaster::clear_targets(&mut compute, &plan);
    let placements = zgui_scene::Placements::of(&scene.spatial);
    let mut frame = zgui_render::VectorFrame::new(
        &plan,
        &scene.primitives.vectors,
        &scene.clips,
        &scene.paints,
        &placements,
    );
    zgui_render::VectorRaster::prepare(&mut compute, &mut frame).expect("nothing failed");
    assert_eq!(
        compute.last_frame().unclippable,
        1,
        "the item under a shapeless clip was left out, and counted"
    );

    let mut fallback = zgui_render_vector_coverage::CoverageRaster::new(&gpu, 128, 128);
    let plan = zgui_render::VectorRaster::plan(&mut fallback, scene.pass_plan());
    zgui_render::VectorRaster::clear_targets(&mut fallback, &plan);
    let placements = zgui_scene::Placements::of(&scene.spatial);
    let mut frame = zgui_render::VectorFrame::new(
        &plan,
        &scene.primitives.vectors,
        &scene.clips,
        &scene.paints,
        &placements,
    );
    zgui_render::VectorRaster::prepare(&mut fallback, &mut frame).expect("nothing failed");
    assert_eq!(
        fallback.last_frame().unclippable,
        1,
        "and the fallback made the same decision, which is what makes it one contract"
    );
}
