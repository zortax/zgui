//! The startup smoke test, over both halves of the renderer.
//!
//! A known pattern is drawn and read back through the quad path *and* through the path renderer, so
//! a driver that mishandles a storage view, or a rasteriser that silently writes nothing, fails
//! loudly at startup rather than showing an empty window.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_render::MemoryReport;
use zgui_scene::{ClipId, Scene};

use support::{Which, harness, opaque, path, present, quad, rect, scene, vector};

/// The four quadrants of the pattern, and the colour each must come back as.
const PATTERN: [((f32, f32), [u8; 3]); 4] = [
    ((0.0, 0.0), [255, 0, 0]),
    ((64.0, 0.0), [0, 255, 0]),
    ((0.0, 64.0), [0, 0, 255]),
    ((64.0, 64.0), [255, 255, 0]),
];

/// Where each quadrant is.
fn quadrant(x: f32, y: f32) -> Rect<DevicePx, Device> {
    rect(x, y, 64.0, 64.0)
}

/// Draws the pattern with `draw`, and checks every quadrant came back.
fn check(which: Which, mut draw: impl FnMut(&mut Scene, Rect<DevicePx, Device>, [u8; 3], u32)) {
    let Some(mut harness) = harness(which) else {
        return;
    };
    let mut scene = scene();
    for (index, ((x, y), colour)) in PATTERN.into_iter().enumerate() {
        draw(&mut scene, quadrant(x, y), colour, index as u32);
    }
    scene.finish(&DamageSet::full());

    let pixels = present(&mut harness.renderer, &scene);
    for ((x, y), colour) in PATTERN {
        let at = (x as i32 + 32, y as i32 + 32);
        assert_eq!(
            pixels.rgba(at.0, at.1),
            [colour[0], colour[1], colour[2], 255],
            "the quadrant at {at:?} did not come back"
        );
    }
}

#[test]
fn the_pattern_comes_back_through_the_quad_path() {
    check(Which::Vello, |scene, bounds, colour, _| {
        quad(scene, bounds, opaque(colour[0], colour[1], colour[2]));
    });
}

#[test]
fn the_pattern_comes_back_through_the_path_renderer() {
    check(Which::Vello, |scene, bounds, colour, index| {
        vector(
            scene,
            index,
            path(bounds),
            opaque(colour[0], colour[1], colour[2]),
            ClipId::ROOT,
        );
    });
}

#[test]
fn the_pattern_comes_back_through_the_fallback_rasteriser() {
    check(Which::Coverage, |scene, bounds, colour, index| {
        vector(
            scene,
            index,
            path(bounds),
            opaque(colour[0], colour[1], colour[2]),
            ClipId::ROOT,
        );
    });
}

/// The two budgets a rasteriser holds are reported apart, because they scale with different things.
#[test]
fn the_fixed_footprint_and_the_scratch_are_two_numbers_and_not_one() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    vector(
        &mut scene,
        0,
        path(rect(16.0, 16.0, 64.0, 64.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());

    let outcome = zgui_render::Renderer::draw(&mut *harness, &scene, &DamageSet::full());
    let memory: MemoryReport = outcome.stats().expect("presented").memory;

    assert!(
        memory.fixed > 64 * 1024 * 1024,
        "the path renderer's fixed buffers are hundreds of megabytes, and this reported {}",
        memory.fixed
    );
    assert!(
        memory.scratch > 0 && memory.scratch < memory.fixed,
        "the scratch is a separate and much smaller budget, and this reported {}",
        memory.scratch
    );
    assert!(
        memory.targets > 0,
        "the renderer's own targets are still counted beside the rasteriser's"
    );
    assert_eq!(
        memory.total(),
        memory.fixed + memory.targets + memory.scratch + memory.atlases + memory.buffers
    );
}

/// The fallback holds nothing fixed at all, which is the whole shape of it against the other one.
#[test]
fn the_fallback_holds_no_fixed_footprint() {
    let Some(mut harness) = harness(Which::Coverage) else {
        return;
    };
    let mut scene = scene();
    vector(
        &mut scene,
        0,
        path(rect(16.0, 16.0, 64.0, 64.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    let outcome = zgui_render::Renderer::draw(&mut *harness, &scene, &DamageSet::full());
    let memory = outcome.stats().expect("presented").memory;
    assert_eq!(memory.fixed, 0);
    assert!(memory.scratch > 0);
}
