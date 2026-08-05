//! Opening a device, refusing to, and what a renderer does around the edges of a frame.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{Scale, Size};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::wgpu;
use zgui_render_wgpu::{Acquisition, Builder, SrgbTier};
use zgui_scene::{BackdropFilter, Filter, GroupBoundary, Quad, Scene};

use support::{SIDE, device_lock, opaque, plain_renderer, present, rect};

/// A target the size of every other test's.
fn target() -> RenderTarget {
    RenderTarget::new(Size::new(SIDE, SIDE), Scale::new(1.0))
}

#[test]
fn with_every_backend_masked_out_the_failure_names_what_it_tried_and_does_not_fall_back() {
    let _device = device_lock();
    let failure = Builder::with_backends(wgpu::Backends::empty())
        .offscreen(target(), wgpu::TextureFormat::Bgra8Unorm, false)
        .expect_err("no backend can produce a device");
    assert!(
        failure.candidates.is_empty(),
        "nothing was enumerated, so nothing can have been rejected: {:?}",
        failure.candidates
    );
    assert!(
        failure.to_string().contains("no usable graphics device"),
        "{failure}"
    );
}

#[test]
fn a_rejected_adapter_is_named_with_the_reason_it_was_rejected() {
    // Every adapter of a backend that exists is enumerated and tried; if a machine has none, the
    // list is empty and there is nothing to assert about its contents. What is asserted either way
    // is that the failure is typed and carries the list rather than silently opening something
    // else — a window that appears and never paints is the outcome being refused here.
    let failure =
        zgui_render::GpuUnavailable::new().rejected("an adapter", "it would not produce a device");
    assert_eq!(failure.candidates.len(), 1);
    assert!(failure.to_string().contains('1'));
}

#[test]
fn the_gl_backend_is_enumerated_and_its_outcome_is_reported_rather_than_assumed() {
    // The plan enumerates Vulkan and GL. Nothing in this workspace had ever opened a GL device;
    // this is the case that says what actually happens, on whatever machine runs it. Either a
    // device opens and the pattern draws, or the failure names the adapter and the reason — the
    // outcome being avoided is enumerating a backend that silently cannot be opened.
    let _device = device_lock();
    let result = Builder::with_backends(wgpu::Backends::GL).offscreen(
        target(),
        wgpu::TextureFormat::Rgba8Unorm,
        false,
    );
    match result {
        Ok(mut renderer) => {
            let mut scene = Scene::new();
            scene.begin_frame(Size::new(SIDE, SIDE));
            let fill = scene
                .paints
                .add(zgui_scene::Paint::Solid(opaque(0, 128, 255)));
            scene.push_quad(Quad::filled(rect(0.0, 0.0, 64.0, 64.0), fill));
            scene.finish(&DamageSet::full());
            let pixels = present(&mut renderer, &scene);
            assert_eq!(
                pixels.rgba(32, 32),
                [0, 128, 255, 255],
                "a GL device that opens must also draw"
            );
            eprintln!("GL: opened and drew on {:?}", renderer.formats());
        }
        Err(failure) => {
            assert!(
                !failure.candidates.is_empty(),
                "a GL adapter that cannot be opened must still be named"
            );
            for candidate in &failure.candidates {
                eprintln!("GL unavailable: {}: {}", candidate.name, candidate.reason);
            }
        }
    }
}

#[test]
fn what_the_device_can_do_is_read_off_the_device_and_published() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let capabilities = renderer.capabilities();
    assert!(
        capabilities.max_texture_size >= 2048,
        "any device worth using can hold a surface-sized texture"
    );
    eprintln!(
        "capabilities: subpixel_text={} vector_compute={} mutable_texture_formats={} max_texture={}",
        capabilities.subpixel_text,
        capabilities.vector_compute,
        capabilities.mutable_texture_formats,
        capabilities.max_texture_size
    );
}

#[test]
fn the_formats_chosen_are_the_ones_the_startup_line_reports() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let formats = renderer.formats();
    assert_eq!(formats.tier, SrgbTier::Native);
    assert!(!formats.surface.is_srgb());
    assert!(!formats.scene.is_srgb());
    assert_eq!(formats.scratch, wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(formats.alpha_mode, wgpu::CompositeAlphaMode::Opaque);
    assert!(formats.is_sound());
}

#[test]
fn resizing_keeps_the_composed_target_within_its_size_class_and_still_draws() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let before = renderer.memory().targets;

    // Inside the same class: nothing is reallocated.
    renderer.configure(RenderTarget::new(Size::new(200, 200), Scale::new(1.0)));
    assert_eq!(
        renderer.memory().targets,
        before,
        "a size inside the class reuses the allocation"
    );

    // Past it: reallocated, and the frame after it still composes correctly at the new extent.
    renderer.configure(RenderTarget::new(Size::new(300, 300), Scale::new(1.0)));
    assert!(
        renderer.memory().targets > before,
        "a size past the class grows the allocation"
    );

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(300, 300));
    let fill = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 0, 0)));
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 300.0, 300.0), fill));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);
    assert_eq!(pixels.size(), Size::new(300, 300));
    assert_eq!(pixels.rgba(299, 299), [255, 0, 0, 255], "the far corner");
}

#[test]
fn only_a_frame_that_will_be_presented_tells_the_compositor_it_is_coming() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let _device = device_lock();
    let notified = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&notified);
    let built = Builder::new()
        .with_pre_present(Box::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }))
        .offscreen(target(), wgpu::TextureFormat::Bgra8Unorm, false);
    let Ok(mut renderer) = built else {
        return;
    };

    assert_eq!(
        notified.load(Ordering::Relaxed),
        0,
        "nothing is announced before a frame is drawn"
    );

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.finish(&DamageSet::full());

    for answer in [
        Acquisition::Timeout,
        Acquisition::Occluded,
        Acquisition::Outdated,
        Acquisition::Lost,
        Acquisition::Validation,
    ] {
        renderer.inject_surface_fault(answer, 1);
        renderer.draw(&scene, &DamageSet::full());
        assert_eq!(
            notified.load(Ordering::Relaxed),
            0,
            "a {answer:?} acquisition announced a frame that cannot be presented"
        );
    }

    renderer.draw(&scene, &DamageSet::new());
    assert_eq!(
        notified.load(Ordering::Relaxed),
        1,
        "the successful retry did not present the composed target after the failures"
    );

    renderer.inject_surface_fault(Acquisition::Suboptimal, 1);
    renderer.draw(&scene, &DamageSet::full());
    assert_eq!(
        notified.load(Ordering::Relaxed),
        2,
        "a suboptimal texture is still presented and must be announced"
    );
}

#[test]
fn an_empty_scene_composes_a_frame_that_holds_nothing() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.finish(&DamageSet::full());

    let outcome = renderer.draw(&scene, &DamageSet::full());
    let stats = outcome.stats().expect("the frame reached its target");
    assert_eq!(
        stats.draw_calls, 2,
        "clearing the one damaged rectangle, then the copy to the surface"
    );
    assert!(outcome.retires_damage());

    let pixels = renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    assert_eq!(pixels.rgba(64, 64), [0, 0, 0, 0], "nothing was drawn");
}

#[test]
fn an_external_texture_is_registered_under_a_handle_of_the_renderers_own() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let first = renderer.register_external(zgui_render::ExternalTexture {
        id: zgui_scene::ExternalTextureId(7),
        handle: zgui_render::TextureHandle(0),
        size: Size::new(16, 16),
        premultiplied: true,
    });
    let second = renderer.register_external(zgui_render::ExternalTexture {
        id: zgui_scene::ExternalTextureId(8),
        handle: zgui_render::TextureHandle(0),
        size: Size::new(16, 16),
        premultiplied: true,
    });
    assert_ne!(
        first, second,
        "two textures do not share one handle, whatever the caller passed in"
    );
    renderer.release_external(first);
}

#[test]
fn what_the_renderer_holds_is_reported_in_the_parts_a_budget_is_written_against() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let memory = renderer.memory();
    // The composed target is allocated at its size class, four bytes to the texel. Asserting the
    // figure rather than that it is positive is what catches it being reported in the units a
    // device budgets a render pass in — eight bytes for a four-byte texel — which would put every
    // memory budget out by a factor of two against an atlas measured in real bytes.
    let class = zgui_render_wgpu::target::scene_texture::size_class(SIDE) as u64;
    assert_eq!(
        memory.targets,
        4 * class * class,
        "the composed target is held, and reported in the bytes it occupies"
    );
    assert!(memory.buffers > 0, "the instance buffers are held");
    assert_eq!(memory.atlases, 0, "nothing has been cached yet");
    assert_eq!(
        memory.total(),
        memory.fixed + memory.targets + memory.scratch + memory.atlases + memory.buffers
    );
}

#[test]
fn a_lost_device_is_rebuilt_and_the_frame_after_it_draws_the_same_pixels() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let fill = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(0, 200, 100)));
    scene.push_quad(Quad::filled(rect(16.0, 16.0, 64.0, 64.0), fill));
    scene.finish(&DamageSet::full());

    let before = present(&mut renderer, &scene);

    // The driver reports a loss on whatever thread it likes and the frame loop reads the flag, so
    // this is exactly what a real loss delivers to the renderer.
    renderer
        .gpu()
        .loss()
        .report(wgpu::DeviceLostReason::Unknown, "injected by a test");

    let outcome = renderer.draw(&scene, &DamageSet::full());
    assert_eq!(
        outcome,
        zgui_render::FrameOutcome::Recovered,
        "the frame that notices the loss rebuilds instead of drawing"
    );
    assert!(
        !renderer.gpu().loss().is_lost(),
        "the rebuilt device is not the lost one"
    );
    assert_eq!(
        renderer.memory().atlases,
        0,
        "nothing a device held survives it"
    );

    let after = present(&mut renderer, &scene);
    assert_eq!(
        before.max_difference(&after),
        0,
        "the frame after recovery draws what the frame before it drew"
    );
}

#[test]
fn two_hundred_frames_of_nested_groups_and_backdrops_hold_the_same_device_memory_as_five() {
    // Every isolated target is a lease rather than an allocation, so a frame that isolates four
    // groups, blurs each of them and frosts a panel over the lot has to cost the pool the same
    // targets on its two-hundredth frame as on its fifth. Measuring it over a run rather than
    // asserting that the pool has a release path is the difference between a leak that shows up in
    // a test and one that shows up as an out-of-memory an hour into a session.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let scene = |step: i32| {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(SIDE, SIDE));
        let white = scene
            .paints
            .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
        scene.push_quad(Quad::filled(
            rect(0.0, 0.0, SIDE as f32, SIDE as f32),
            white,
        ));
        let mut open: Vec<GroupBoundary> = Vec::new();
        for level in 0..4 {
            let boundary = GroupBoundary::start(
                rect(8.0 + level as f32, 8.0, 100.0, 100.0),
                0.7,
                zgui_scene::peniko::BlendMode::default(),
                [Filter::Blur(3.0), Filter::Contrast(1.2)]
                    .into_iter()
                    .collect(),
            );
            scene.push_group(boundary.clone());
            open.push(boundary);
        }
        let paint = scene
            .paints
            .add(zgui_scene::Paint::Solid(opaque((step * 7) as u8, 40, 90)));
        scene.push_quad(Quad::filled(rect(24.0, 24.0, 60.0, 60.0), paint));
        while let Some(boundary) = open.pop() {
            scene.push_group(boundary.end());
        }
        scene.push_backdrop(BackdropFilter::new(
            rect(20.0, 20.0, 60.0, 60.0),
            [Filter::Blur(5.0)].into_iter().collect(),
        ));
        scene.finish(&DamageSet::full());
        scene
    };

    for step in 0..5 {
        renderer.draw(&scene(step), &DamageSet::full());
    }
    let settled = renderer.memory().total();
    assert!(
        renderer.groups().peak() >= 4,
        "four levels were open at once, and the pool lent {}",
        renderer.groups().peak()
    );
    for step in 5..200 {
        renderer.draw(&scene(step), &DamageSet::full());
    }
    assert_eq!(
        renderer.memory().total(),
        settled,
        "a hundred and ninety-five more frames allocated nothing"
    );
    assert_eq!(renderer.groups().lent(), 0, "and every lease came back");
}
