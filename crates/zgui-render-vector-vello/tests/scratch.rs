//! What the scratch texture costs, and when it gives the memory back.

mod support;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_render::VectorRaster;
use zgui_render_vector_coverage::CoverageRaster;
use zgui_render_vector_vello::VelloRaster;
use zgui_render_vector_vello::raster::scratch::Scratch;
use zgui_scene::{ClipId, Scene};

use support::{Which, harness_at, opaque, path, quad, rect, scene_at, vector};

/// The extent every scene here fits in.
const SIDE: i32 = 640;

/// A scene whose passes reach the far corner of the surface and stack `deep` high at the near one.
fn wide(deep: u32) -> Scene {
    let mut scene = scene_at(SIDE);
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    for index in 0..deep {
        vector(
            &mut scene,
            index,
            path(rect(16.0, 16.0, 24.0, 24.0)),
            opaque(255, 255, 255),
            ClipId::ROOT,
        );
        quad(
            &mut scene,
            rect(16.0, 16.0, 24.0, 24.0),
            Color::srgb_u8(0, 0, 0, 0),
        );
    }
    vector(
        &mut scene,
        deep,
        path(rect(SIDE as f32 - 32.0, SIDE as f32 - 32.0, 24.0, 24.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    scene
}

/// A scene of one small pass in the near corner.
fn narrow() -> Scene {
    let mut scene = scene_at(SIDE);
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    vector(
        &mut scene,
        0,
        path(rect(0.0, 0.0, 8.0, 8.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    scene
}

/// A frame that needs less than the last one eventually gets a texture that costs less.
///
/// The defect this closes is a texture that only ever grew: one frame that needed a large one held
/// the memory until the process ended, and nothing anywhere read that the frame was long over.
#[test]
fn the_scratch_shrinks_when_the_content_does() {
    let Some(harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    let gpu = std::sync::Arc::clone(harness.gpu());
    let mut raster = VelloRaster::new(&gpu, SIDE as u32, SIDE as u32).expect("a rasteriser");

    let large = wide(6);
    raster.plan(large.pass_plan());
    let grown = raster.extent();
    assert!(
        grown > (16, 16),
        "the fixture has to reach further than the narrow one, or nothing here shrinks: {grown:?}"
    );
    assert!(
        raster.layers() > Scratch::LAYERS,
        "the fixture has to stack passes deeper than the floor, or nothing here shrinks"
    );

    let small = narrow();
    raster.plan(small.pass_plan());
    assert!(raster.release_idle_resources() > 0);
    raster.plan(small.pass_plan());
    assert_eq!(
        raster.extent(),
        (16, 16),
        "a settled window has to get its video memory back"
    );
    assert_eq!(
        raster.layers(),
        Scratch::LAYERS,
        "the depth has to come back down with the extent"
    );
}

/// A scroll does not reallocate the scratch, however cheap most of its frames are.
///
/// The demand of a fling swings frame to frame, and a texture thrown away on the quiet frame between
/// two busy ones is bought again by the next busy one. What the wait is measured against is the
/// window's own maximum, not this frame's.
#[test]
fn the_scratch_does_not_shrink_inside_a_fling() {
    let Some(harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    let gpu = std::sync::Arc::clone(harness.gpu());
    let mut raster = VelloRaster::new(&gpu, SIDE as u32, SIDE as u32).expect("a rasteriser");

    let large = wide(6);
    let small = narrow();
    raster.plan(large.pass_plan());
    let held = (raster.extent(), raster.layers());
    for frame in 0..600 {
        let scene = if frame % 30 == 0 { &large } else { &small };
        raster.plan(scene.pass_plan());
        assert_eq!(
            (raster.extent(), raster.layers()),
            held,
            "the scratch was reallocated at frame {frame} of a fling"
        );
    }
}

/// The fallback rasteriser costs twice what the other one does for the same frame, and shares layers
/// on the same terms.
///
/// It holds an accumulation texture and a straight one, so every texel of its scratch is two. That
/// is the whole of why the layer count matters more here — and it had never been measured.
#[test]
fn the_coverage_scratch_is_two_textures_of_the_shared_shape() {
    let Some(harness) = harness_at(SIDE, Which::Coverage) else {
        return;
    };
    let gpu = std::sync::Arc::clone(harness.gpu());
    let mut raster = CoverageRaster::new(&gpu, SIDE as u32, SIDE as u32);

    let scene = wide(2);
    raster.plan(scene.pass_plan());
    let (width, height) = raster.extent();
    assert_eq!(
        raster.scratch_bytes(),
        2 * u64::from(width) * u64::from(height) * u64::from(raster.layers()) * 4,
        "the fallback's memory report has to move with the depth it actually allocated"
    );
    assert_eq!(
        raster.depth(),
        2,
        "one stacked pair costs two layers and the distant pass rejoins the first, exactly as it \
         does for the other rasteriser"
    );
}
