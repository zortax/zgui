//! Presenting into textures the caller owns and rotates between.
//!
//! Ordinary textures stand in for a display controller's scanout buffers here. What is asserted is
//! that a frame lands on the buffer the caller chose and on no other, which is why a backend hands
//! its own buffers in at all. A real one wraps memory the kernel can put on a screen, and nothing
//! below this line can tell the difference.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::{Device, Scale, Size};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::renderer::readback;
use zgui_render_wgpu::target::swapchain::Supplied;
use zgui_render_wgpu::{Gpu, Pixels, SharedGraphics, WgpuRenderer, wgpu};
use zgui_scene::{Quad, Scene};

use support::{SIDE, device_lock, opaque, present, rect};

/// What every texture here is: eight bits a channel, blue first, and no encoding.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// Returns the extent every texture and every target here has.
fn extent() -> Size<i32, Device> {
    Size::new(SIDE, SIDE)
}

/// Returns a target the size of every other test's.
fn target() -> RenderTarget {
    RenderTarget::new(extent(), Scale::new(1.0))
}

/// Returns a texture standing in for one of a display controller's buffers.
///
/// `COPY_SRC` is here for the readbacks below alone. A backend that scans these out never asks for
/// it, because it never reads one back.
fn buffer(gpu: &Gpu, size: Size<i32, Device>, format: wgpu::TextureFormat) -> wgpu::Texture {
    gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("test.scanout"),
        size: wgpu::Extent3d {
            width: size.width as u32,
            height: size.height as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Returns the shared device, or `None` when this machine has none.
///
/// Skipping out loud instead of failing: these tests assert what a device does, and a machine
/// without one has nothing to say about it.
fn open(graphics: &SharedGraphics) -> Option<Arc<Gpu>> {
    match graphics.open_gpu() {
        Ok(gpu) => Some(gpu),
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            None
        }
    }
}

/// Returns a renderer over `count` supplied textures, and the textures themselves.
///
/// The caller keeps its own handles, as the real one does: the buffers outlive any one renderer,
/// and reading them back here is how a frame is proved to have landed on one of them.
fn supplied(graphics: &SharedGraphics, count: usize) -> Option<(WgpuRenderer, Vec<wgpu::Texture>)> {
    let gpu = open(graphics)?;
    let textures: Vec<wgpu::Texture> = (0..count).map(|_| buffer(&gpu, extent(), FORMAT)).collect();
    let renderer = graphics
        .renderer_supplied(target(), textures.clone())
        .expect("a device is open and the textures agree about everything");
    Some((renderer, textures))
}

/// Returns a scene holding one quad of `colour` over the whole target.
fn filled(colour: zgui_color::Color) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(extent());
    let paint = scene.paints.add(zgui_scene::Paint::Solid(colour));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        paint,
    ));
    scene.finish(&DamageSet::full());
    scene
}

/// Reads one of the caller's own textures back, whatever the renderer is pointing at.
fn read(renderer: &WgpuRenderer, texture: &wgpu::Texture) -> Pixels {
    readback::read(renderer.gpu(), texture, FORMAT, extent())
}

/// Returns the pixel every assertion here looks at.
fn centre(pixels: &Pixels) -> [u8; 4] {
    pixels.rgba(SIDE / 2, SIDE / 2)
}

#[test]
fn a_frame_lands_in_the_slot_the_caller_chose_and_in_no_other() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some((mut renderer, textures)) = supplied(&graphics, 2) else {
        return;
    };

    assert!(renderer.present_into(0), "slot 0 of two exists");
    let first = present(&mut renderer, &filled(opaque(255, 0, 0)));
    assert!(renderer.present_into(1), "slot 1 of two exists");
    let second = present(&mut renderer, &filled(opaque(0, 0, 255)));

    assert_eq!(centre(&first), [255, 0, 0, 255]);
    assert_eq!(centre(&second), [0, 0, 255, 255]);
    // The property everything here rests on, read from the caller's own handles after both frames:
    // a second frame on a rotating set must leave the buffer the display controller is reading
    // exactly as it was.
    assert_eq!(
        centre(&read(&renderer, &textures[0])),
        [255, 0, 0, 255],
        "the second frame overwrote the texture the first one was drawn into"
    );
    assert_eq!(
        centre(&read(&renderer, &textures[1])),
        [0, 0, 255, 255],
        "the second frame did not reach the texture it was pointed at"
    );
}

#[test]
fn choosing_a_slot_again_overwrites_it() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some((mut renderer, textures)) = supplied(&graphics, 2) else {
        return;
    };

    assert!(renderer.present_into(0));
    present(&mut renderer, &filled(opaque(255, 0, 0)));
    assert!(renderer.present_into(1));
    present(&mut renderer, &filled(opaque(0, 0, 255)));
    // Back to the first buffer, as a rotating set does every second frame.
    assert!(renderer.present_into(0));
    present(&mut renderer, &filled(opaque(0, 255, 0)));

    assert_eq!(centre(&read(&renderer, &textures[0])), [0, 255, 0, 255]);
    assert_eq!(
        centre(&read(&renderer, &textures[1])),
        [0, 0, 255, 255],
        "returning to the first buffer disturbed the second"
    );
}

#[test]
fn a_slot_outside_the_set_is_refused_and_the_frame_goes_where_it_was_going() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some((mut renderer, textures)) = supplied(&graphics, 2) else {
        return;
    };

    assert!(renderer.present_into(0));
    present(&mut renderer, &filled(opaque(255, 0, 0)));
    assert!(renderer.present_into(1));
    assert!(
        !renderer.present_into(2),
        "a set of two has no third slot to answer for"
    );
    // Nothing wraps: the frame lands where the last accepted choice pointed. A wrapped slot would
    // have put it on the buffer holding the frame before it.
    present(&mut renderer, &filled(opaque(0, 255, 0)));

    assert_eq!(
        centre(&read(&renderer, &textures[0])),
        [255, 0, 0, 255],
        "a refused slot wrapped round to the first texture"
    );
    assert_eq!(centre(&read(&renderer, &textures[1])), [0, 255, 0, 255]);
}

#[test]
fn a_set_that_cannot_be_presented_to_as_one_is_refused() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some(gpu) = open(&graphics) else {
        return;
    };

    assert!(
        Supplied::new(Vec::new(), extent()).is_none(),
        "an empty set states no format and no extent to derive one from"
    );
    assert!(
        Supplied::new(
            vec![
                buffer(&gpu, extent(), FORMAT),
                buffer(&gpu, extent(), wgpu::TextureFormat::Rgba8Unorm),
            ],
            extent(),
        )
        .is_none(),
        "one set has one format, and these two would be presented in different ones"
    );
    assert!(
        Supplied::new(
            vec![
                buffer(&gpu, extent(), FORMAT),
                buffer(&gpu, Size::new(SIDE / 2, SIDE), FORMAT),
            ],
            extent(),
        )
        .is_none(),
        "one set has one extent, and the frame that landed on the smaller one would be cut"
    );
    assert!(
        Supplied::new(
            vec![buffer(&gpu, extent(), FORMAT)],
            Size::new(SIDE / 2, SIDE)
        )
        .is_none(),
        "the extent stated and the extent the texture has are one extent"
    );

    let mut agreeing = Supplied::new(
        vec![
            buffer(&gpu, extent(), FORMAT),
            buffer(&gpu, extent(), FORMAT),
        ],
        extent(),
    )
    .expect("two textures agreeing about everything are one set");
    assert_eq!(agreeing.len(), 2);
    assert_eq!(agreeing.selected(), 0, "the first is written next");
    assert!(!agreeing.select(2), "a set of two has no third slot");
    assert_eq!(
        agreeing.selected(),
        0,
        "a refused slot moved the selection anyway"
    );
    assert!(agreeing.select(1));
    assert_eq!(agreeing.selected(), 1);
}

#[test]
fn a_resize_leaves_the_supplied_textures_as_they_were() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some((mut renderer, textures)) = supplied(&graphics, 2) else {
        return;
    };

    assert!(renderer.present_into(0));
    present(&mut renderer, &filled(opaque(255, 0, 0)));
    // A display's mode holds still while a program runs, so this is a caller asking for something
    // the renderer cannot do: it did not create these textures and cannot create another.
    renderer.configure(RenderTarget::new(
        Size::new(SIDE / 2, SIDE / 2),
        Scale::new(1.0),
    ));

    for texture in &textures {
        assert_eq!(texture.width(), SIDE as u32, "a texture was reallocated");
        assert_eq!(texture.height(), SIDE as u32, "a texture was reallocated");
    }
    assert_eq!(
        centre(&read(&renderer, &textures[0])),
        [255, 0, 0, 255],
        "the buffer the display controller is reading lost its frame"
    );
}

#[test]
fn a_supplied_renderer_cannot_rebuild_itself_after_a_loss() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some((mut renderer, _textures)) = supplied(&graphics, 2) else {
        return;
    };

    let failure = renderer
        .recover()
        .expect_err("the renderer did not create these textures and cannot create another set");
    assert!(
        failure
            .candidates
            .iter()
            .any(|candidate| candidate.reason.contains("supplied them")),
        "the refusal does not say whose job the new textures are: {:?}",
        failure.candidates
    );
}
