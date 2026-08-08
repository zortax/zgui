//! Several windows on one graphics device.
//!
//! The offscreen path stands in for windows here, because a surface needs a native window and these
//! tests need neither: what is being asserted is that two renderers made from one
//! [`SharedGraphics`] land on one device, draw independently, and converge on one replacement
//! device when that one dies.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::{Scale, Size};
use zgui_render::{FrameOutcome, RenderTarget, Renderer};
use zgui_render_wgpu::wgpu;
use zgui_render_wgpu::{SharedGraphics, WgpuRenderer};
use zgui_scene::{Quad, Scene};

use support::{SIDE, device_lock, opaque, present, rect};

/// A target the size of every other test's.
fn target() -> RenderTarget {
    RenderTarget::new(Size::new(SIDE, SIDE), Scale::new(1.0))
}

/// Two renderers on one shared device, or `None` when this machine has no device at all.
///
/// Skipping out loud rather than failing: these tests assert what a device does, and a machine
/// without one has nothing to say about it.
fn pair(graphics: &SharedGraphics) -> Option<(WgpuRenderer, WgpuRenderer)> {
    let first = match graphics.renderer_offscreen(target(), wgpu::TextureFormat::Bgra8Unorm, false)
    {
        Ok(renderer) => renderer,
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            return None;
        }
    };
    let second = graphics
        .renderer_offscreen(target(), wgpu::TextureFormat::Bgra8Unorm, false)
        .expect("a device that opened once opens a second renderer");
    Some((first, second))
}

/// A scene holding one quad of `colour` over a target `side` pixels square.
fn filled(colour: zgui_color::Color, side: i32) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(side, side));
    let paint = scene.paints.add(zgui_scene::Paint::Solid(colour));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, side as f32, side as f32),
        paint,
    ));
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn two_windows_draw_on_one_device() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some((first, second)) = pair(&graphics) else {
        return;
    };

    // The whole point: a second window costs a swap chain and a composed target, not a driver
    // connection, a queue and another copy of every pipeline.
    assert!(
        Arc::ptr_eq(first.gpu(), second.gpu()),
        "two renderers from one SharedGraphics opened two devices"
    );
    assert!(
        graphics
            .gpu()
            .is_some_and(|gpu| Arc::ptr_eq(&gpu, first.gpu())),
        "the shared device is not the one the renderers were given"
    );
}

#[test]
fn each_window_composes_its_own_pixels() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some((mut first, mut second)) = pair(&graphics) else {
        return;
    };

    let red = opaque(255, 0, 0);
    let blue = opaque(0, 0, 255);
    let drawn_first = present(&mut first, &filled(red, SIDE));
    let drawn_second = present(&mut second, &filled(blue, SIDE));

    // Sharing a device must not mean sharing a target: one window's frame appearing in another's
    // is the failure this asserts against.
    assert_eq!(drawn_first.rgba(SIDE / 2, SIDE / 2), [255, 0, 0, 255]);
    assert_eq!(drawn_second.rgba(SIDE / 2, SIDE / 2), [0, 0, 255, 255]);
}

#[test]
fn windows_whose_displays_chose_different_formats_share_the_pipelines() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let first = match graphics.renderer_offscreen(target(), wgpu::TextureFormat::Bgra8Unorm, false)
    {
        Ok(renderer) => renderer,
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            return;
        }
    };
    // The two-monitor case, on one machine: pipelines are keyed by format as well as by kind, so
    // the second format adds entries to one map rather than needing a device of its own.
    let mut second = graphics
        .renderer_offscreen(target(), wgpu::TextureFormat::Rgba8Unorm, false)
        .expect("a second format is another key, not another device");
    assert!(Arc::ptr_eq(first.gpu(), second.gpu()));

    let green = opaque(0, 255, 0);
    assert_eq!(
        present(&mut second, &filled(green, SIDE)).rgba(SIDE / 2, SIDE / 2),
        [0, 255, 0, 255],
        "a renderer on the second format drew nothing"
    );
}

#[test]
fn resizing_one_window_leaves_the_other_at_its_own_size() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some((mut first, mut second)) = pair(&graphics) else {
        return;
    };

    let half = RenderTarget::new(Size::new(SIDE / 2, SIDE / 2), Scale::new(1.0));
    first.configure(half);

    let red = opaque(255, 0, 0);
    let blue = opaque(0, 0, 255);
    let drawn_first = present(&mut first, &filled(red, SIDE / 2));
    let drawn_second = present(&mut second, &filled(blue, SIDE));

    assert_eq!(drawn_first.size().width, SIDE / 2);
    assert_eq!(
        drawn_second.size().width,
        SIDE,
        "one window's resize resized another"
    );
    assert_eq!(drawn_second.rgba(SIDE / 2, SIDE / 2), [0, 0, 255, 255]);
}

#[test]
fn a_lost_device_is_replaced_once_for_every_window_on_it() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some((mut first, mut second)) = pair(&graphics) else {
        return;
    };
    let dead = Arc::as_ptr(first.gpu());

    // One loss, seen by every window on the device.
    first
        .gpu()
        .loss()
        .report(wgpu::DeviceLostReason::Unknown, "test");
    assert!(second.gpu().loss().is_lost());

    let white = opaque(255, 255, 255);
    let scene = filled(white, SIDE);
    assert!(matches!(
        first.draw(&scene, &DamageSet::full()),
        FrameOutcome::Recovered
    ));
    assert!(matches!(
        second.draw(&scene, &DamageSet::full()),
        FrameOutcome::Recovered
    ));

    // Both rebuilt, and onto the *same* new device: one replacement opened by whichever window
    // noticed first and handed to the rest, rather than one device per window.
    assert!(
        Arc::ptr_eq(first.gpu(), second.gpu()),
        "the windows came out of the loss on different devices"
    );
    assert_ne!(
        Arc::as_ptr(first.gpu()),
        dead,
        "the renderers are still on the device that died"
    );
    assert_eq!(
        present(&mut first, &scene).rgba(SIDE / 2, SIDE / 2),
        [255, 255, 255, 255],
        "a rebuilt renderer draws nothing"
    );
}

#[test]
fn with_every_backend_masked_out_no_window_gets_a_device() {
    // Parity with the builder's contract: an empty backend set is how a machine with no usable
    // graphics device is reproduced on a machine that has one.
    let _device = device_lock();
    let graphics = SharedGraphics::with_backends(wgpu::Backends::empty());
    let failure = graphics
        .renderer_offscreen(target(), wgpu::TextureFormat::Bgra8Unorm, false)
        .expect_err("no backend can produce a device");
    assert!(
        failure.candidates.is_empty(),
        "nothing was enumerated, so nothing can have been rejected: {:?}",
        failure.candidates
    );
    assert!(
        graphics.gpu().is_none(),
        "a failed open left a device behind"
    );
}
