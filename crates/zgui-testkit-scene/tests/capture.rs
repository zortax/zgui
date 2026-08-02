//! What the capture renderer reports, and what it refuses to let a test believe.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{Css, Device, Scale, Size};
use zgui_render::{ExternalTexture, FrameOutcome, RenderTarget, Renderer, TextureHandle};
use zgui_scene::ExternalTextureId;
use zgui_testkit_scene::CaptureRenderer;

use crate::support::kitchen_sink;

#[test]
fn a_frame_drawn_here_reports_no_draw_calls_at_all() {
    let mut renderer = CaptureRenderer::new();
    let outcome = renderer.draw(&kitchen_sink(), &DamageSet::full());

    let stats = outcome.stats().expect("a capture frame always presents");
    assert_eq!(stats.draw_calls, 0);
    assert_eq!(stats.damage_px, 0);
    assert_eq!(stats.bytes_uploaded, 0);
    assert_eq!(stats.memory, zgui_render::MemoryReport::ZERO);
}

#[test]
fn the_vector_pass_count_is_the_scenes_own_decision() {
    let scene = kitchen_sink();
    let planned = scene.pass_plan().passes.len() as u32;
    assert!(planned > 0, "the kitchen sink has vector content");

    let mut renderer = CaptureRenderer::new();
    let stats = renderer
        .draw(&scene, &DamageSet::full())
        .stats()
        .expect("presented");
    assert_eq!(stats.vector_passes, planned);
}

#[test]
fn every_frame_presents_and_every_frame_is_recorded() {
    let mut renderer = CaptureRenderer::new();
    assert!(renderer.transcript().is_none());

    for expected in 1..=3 {
        let outcome = renderer.draw(&kitchen_sink(), &DamageSet::full());
        assert!(matches!(outcome, FrameOutcome::Presented(_)));
        assert!(outcome.retires_damage());
        assert!(!outcome.wants_another_frame());
        assert_eq!(renderer.frames(), expected);
    }
    assert!(renderer.transcript().is_some());
}

#[test]
fn configuring_a_surface_is_remembered_and_changes_nothing_else() {
    let mut renderer = CaptureRenderer::new();
    assert!(renderer.target().is_none());

    let target = RenderTarget::new(Size::new(320, 200), Scale::<Css, Device>::new(2.0));
    renderer.configure(target);
    assert_eq!(renderer.target(), Some(target));
    assert!(!renderer.capabilities().subpixel_text);
}

#[test]
fn an_external_texture_registration_survives_a_release_of_something_else() {
    let mut renderer = CaptureRenderer::new();
    let first = renderer.register_external(texture(1));
    let second = renderer.register_external(texture(2));
    assert_ne!(first, second);

    renderer.release_external(first);
    assert!(renderer.externals().get(first).is_none());
    assert_eq!(
        renderer.externals().get(second).map(|held| held.id.0),
        Some(2)
    );
    assert_eq!(renderer.externals().len(), 1);
}

/// A texture to register.
fn texture(id: u64) -> ExternalTexture {
    ExternalTexture {
        id: ExternalTextureId(id),
        handle: TextureHandle(0),
        size: Size::new(16, 16),
        premultiplied: true,
    }
}
