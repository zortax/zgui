//! GPU pass timing: what a timed frame reports, on a device that has timestamp queries.
//!
//! A test binary of its own, because timing has to be asked for before the device opens and the
//! request holds for the whole process.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::Size;
use zgui_render_wgpu::{GpuTiming, wgpu};
use zgui_scene::{GroupBoundary, Quad, Scene};

use support::{SIDE, opaque, plain_renderer, present, rect};

/// A frame with a plain quad and a half-transparent group, so that it has draws, a group and a
/// blit.
fn scene() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    let boundary = GroupBoundary::start(
        rect(16.0, 16.0, 96.0, 96.0),
        0.5,
        zgui_scene::peniko::BlendMode::default(),
        Default::default(),
    );
    scene.push_group(boundary.clone());
    let black = scene.paints.add(zgui_scene::Paint::Solid(opaque(0, 0, 0)));
    scene.push_quad(Quad::filled(rect(16.0, 16.0, 64.0, 64.0), black));
    scene.push_group(boundary.end());
    scene.finish(&DamageSet::full());
    scene
}

/// Every figure of `timing` is a finite, non-negative number of milliseconds.
fn assert_sane(timing: &GpuTiming) {
    for (name, ms) in [
        ("draws", timing.draws_ms),
        ("groups", timing.groups_ms),
        ("vectors", timing.vectors_ms),
        ("blit", timing.blit_ms),
        ("total", timing.total_ms()),
    ] {
        assert!(
            ms.is_finite() && ms >= 0.0,
            "{name} is {ms} ms in {timing:?}"
        );
    }
    assert_eq!(timing.untimed, 0, "a small frame fits the query set");
}

#[test]
fn a_timed_frame_reports_non_negative_gpu_time_per_kind() {
    zgui_render_wgpu::gpu::timing::enable();
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let features = renderer.gpu().device().features();
    if !features.contains(wgpu::Features::TIMESTAMP_QUERY) {
        assert!(
            !renderer.gpu_timing_enabled(),
            "a device without timestamp queries times nothing"
        );
        eprintln!("skipped: the adapter has no timestamp queries");
        return;
    }
    assert!(renderer.gpu_timing_enabled());
    assert_eq!(renderer.gpu_timed_frames(), Some(0));
    assert_eq!(renderer.gpu_timing(), None);

    let scene = scene();
    present(&mut renderer, &scene);
    present(&mut renderer, &scene);
    assert_eq!(renderer.gpu_timed_frames(), Some(2));

    renderer.gpu().wait();
    let timings = renderer.take_gpu_timings();
    assert_eq!(
        timings
            .iter()
            .map(|timing| timing.frame)
            .collect::<Vec<_>>(),
        [0, 1],
        "both frames got a query set and both results arrived after the wait: {timings:?}"
    );
    for timing in &timings {
        assert_sane(timing);
    }
    assert_eq!(renderer.gpu_timing(), timings.last().copied());
    assert!(
        renderer.take_gpu_timings().is_empty(),
        "a result is handed over once"
    );
    eprintln!("GPU timing on {}: {timings:?}", renderer.gpu().describe());
}
