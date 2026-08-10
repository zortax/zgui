//! Presenting into textures the caller owns and rotates between.
//!
//! Ordinary textures stand in for a display controller's scanout buffers here. What is asserted is
//! that a frame lands on the buffer the caller chose and on no other, which is why a backend hands
//! its own buffers in at all. A real one wraps memory the kernel can put on a screen, and nothing
//! below this line can tell the difference.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, Scale, Size};
use zgui_render::{FrameOutcome, RenderTarget, Renderer, SkipReason};
use zgui_render_wgpu::renderer::readback;
use zgui_render_wgpu::target::swapchain::Supplied;
use zgui_render_wgpu::{Gpu, Pixels, SharedGraphics, SrgbTier, WgpuRenderer, wgpu};
use zgui_scene::{Quad, Scene};

use support::{SIDE, device_lock, opaque, present, rect};

/// What every texture here is: eight bits a channel, blue first, and no encoding.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// The usage a scanout buffer needs, plus the one a readback here needs.
///
/// `COPY_SRC` is for the readbacks alone. A backend that scans these out never asks for it, because
/// it never reads one back.
const USAGE: wgpu::TextureUsages =
    wgpu::TextureUsages::RENDER_ATTACHMENT.union(wgpu::TextureUsages::COPY_SRC);

/// One property a scanout buffer has to have, named, and the change that takes it away.
type Fatal = (&'static str, fn(&mut wgpu::TextureDescriptor<'_>));

/// Returns the extent every texture and every target here has.
fn extent() -> Size<i32, Device> {
    Size::new(SIDE, SIDE)
}

/// Returns a target the size of every other test's.
fn target() -> RenderTarget {
    RenderTarget::new(extent(), Scale::new(1.0))
}

/// Returns a texture standing in for one of a display controller's buffers.
fn buffer(gpu: &Gpu, size: Size<i32, Device>, format: wgpu::TextureFormat) -> wgpu::Texture {
    described(gpu, |descriptor| {
        descriptor.size.width = size.width as u32;
        descriptor.size.height = size.height as u32;
        descriptor.format = format;
    })
}

/// Returns a texture that is a scanout buffer except for whatever `alter` changes about it.
///
/// The refusals below are each one altered property, so the description they start from has to be
/// the one that is accepted.
fn described(gpu: &Gpu, alter: impl FnOnce(&mut wgpu::TextureDescriptor<'_>)) -> wgpu::Texture {
    let mut descriptor = wgpu::TextureDescriptor {
        label: Some("test.scanout"),
        size: wgpu::Extent3d {
            width: SIDE as u32,
            height: SIDE as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: USAGE,
        view_formats: &[],
    };
    alter(&mut descriptor);
    gpu.device().create_texture(&descriptor)
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
    supplied_in(graphics, count, FORMAT)
}

/// The same, in a format of the caller's choosing.
fn supplied_in(
    graphics: &SharedGraphics,
    count: usize,
    format: wgpu::TextureFormat,
) -> Option<(WgpuRenderer, Vec<wgpu::Texture>)> {
    let gpu = open(graphics)?;
    let textures: Vec<wgpu::Texture> = (0..count).map(|_| buffer(&gpu, extent(), format)).collect();
    let renderer = graphics
        .renderer_supplied(target(), textures.clone())
        .expect("a device is open and the textures agree about everything");
    Some((renderer, textures))
}

/// Returns a scene holding one quad of `colour` over the whole target.
fn filled(colour: Color) -> Scene {
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

/// Returns opaque white with half-transparent grey over it, as the encoding suite measures with.
///
/// Unencoded, source-over gives 191. Against an encoded attachment the same blend happens in
/// linear light and gives 225, which is the error a supplied texture must not reintroduce.
fn half_grey_over_white() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(extent());
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(Color::srgb_u8(255, 255, 255, 255)));
    let grey = scene
        .paints
        .add(zgui_scene::Paint::Solid(Color::srgb_u8(128, 128, 128, 128)));
    let whole = rect(0.0, 0.0, SIDE as f32, SIDE as f32);
    scene.push_quad(Quad::filled(whole, white));
    scene.push_quad(Quad::filled(whole, grey));
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
    // have picked slot 0 for no reason but arithmetic, and which buffers are free is the caller's
    // knowledge alone.
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
        Supplied::new(Vec::new()).is_none(),
        "an empty set states no format and no extent to derive one from"
    );
    assert!(
        Supplied::new(vec![
            buffer(&gpu, extent(), FORMAT),
            buffer(&gpu, extent(), wgpu::TextureFormat::Rgba8Unorm),
        ])
        .is_none(),
        "one set has one format, and these two would be presented in different ones"
    );
    assert!(
        Supplied::new(vec![
            buffer(&gpu, extent(), FORMAT),
            buffer(&gpu, Size::new(SIDE / 2, SIDE), FORMAT),
        ])
        .is_none(),
        "one set has one extent, and the frame that landed on the smaller one would be cut"
    );

    // The five a handle answers and wgpu stops the process over. Each is refused on its own, from
    // a description that is otherwise the accepted one.
    let fatal: [Fatal; 5] = [
        ("no RENDER_ATTACHMENT", |descriptor| {
            descriptor.usage = wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC;
        }),
        ("mip levels", |descriptor| descriptor.mip_level_count = 2),
        ("multisampled", |descriptor| descriptor.sample_count = 4),
        ("layered", |descriptor| {
            descriptor.size.depth_or_array_layers = 2;
        }),
        ("three-dimensional", |descriptor| {
            descriptor.dimension = wgpu::TextureDimension::D3;
            descriptor.size.depth_or_array_layers = 2;
        }),
    ];
    for (named, alter) in fatal {
        let odd = described(&gpu, alter);
        assert!(
            Supplied::unusable(std::slice::from_ref(&odd)).is_some(),
            "{named}: a texture a frame cannot be copied into was accepted"
        );
        assert!(
            Supplied::new(vec![odd]).is_none(),
            "{named}: a texture a frame cannot be copied into was accepted"
        );
    }

    let usable = vec![
        buffer(&gpu, extent(), FORMAT),
        buffer(&gpu, extent(), FORMAT),
    ];
    assert!(
        Supplied::unusable(&usable).is_none(),
        "two ordinary scanout buffers were refused"
    );
    let mut agreeing =
        Supplied::new(usable).expect("two textures agreeing about everything are one set");
    assert_eq!(agreeing.len(), 2);
    assert_eq!(agreeing.size(), extent(), "the extent is the textures' own");
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
fn a_target_and_a_texture_set_that_state_different_extents_are_refused() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some(gpu) = open(&graphics) else {
        return;
    };

    // Two things describing one screen, supplied separately. Letting them disagree would put a
    // stretched frame on a display every frame, with nothing to say so.
    let failure = graphics
        .renderer_supplied(
            RenderTarget::new(Size::new(SIDE / 2, SIDE), Scale::new(1.0)),
            vec![buffer(&gpu, extent(), FORMAT)],
        )
        .expect_err("a target and a texture set of different extents are not one screen");
    assert!(
        failure
            .candidates
            .iter()
            .any(|candidate| candidate.reason.contains("one screen has one extent")),
        "the refusal does not name the disagreement: {:?}",
        failure.candidates
    );
}

#[test]
fn textures_made_on_a_device_another_graphics_never_opened_are_refused() {
    let _device = device_lock();
    let opened = SharedGraphics::new();
    let Some(gpu) = open(&opened) else {
        return;
    };
    let textures = vec![buffer(&gpu, extent(), FORMAT)];

    // Nothing can check this at run time: a wgpu texture states no device, so a set made on one
    // device and presented through another is a validation failure at the first frame and a
    // stopped process. The order prevents it, and this refusal states the order.
    let fresh = SharedGraphics::new();
    let failure = fresh
        .renderer_supplied(target(), textures)
        .expect_err("no device is open, so nothing can present into textures made on one");
    assert!(
        failure
            .candidates
            .iter()
            .any(|candidate| candidate.reason.contains("open_gpu")),
        "the refusal does not say how to get a device first: {:?}",
        failure.candidates
    );
}

#[test]
fn an_encoded_supplied_texture_still_receives_the_bytes_that_were_composed() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    // A caller whose scanout buffers are encoded. The set claims no unencoded twin, because the
    // view formats of a texture are fixed when the texture is created and this one already exists,
    // so the encode has to be cancelled in the copy that ends the frame.
    let Some((mut renderer, _textures)) =
        supplied_in(&graphics, 1, wgpu::TextureFormat::Bgra8UnormSrgb)
    else {
        return;
    };
    let formats = renderer.formats();
    assert_eq!(formats.tier, SrgbTier::UndoInBlit);
    assert_eq!(
        formats.scene,
        wgpu::TextureFormat::Bgra8Unorm,
        "the composed target never encodes"
    );
    assert_eq!(
        formats.present_attachment(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        "a supplied texture is rendered through its own format, so the pipeline is built for it"
    );

    let presented = present(&mut renderer, &half_grey_over_white());
    let composed = renderer.read_composed();
    assert_eq!(
        composed.rgba(SIDE / 2, SIDE / 2),
        [191, 191, 191, 255],
        "the blend still happens unencoded"
    );
    assert!(
        composed.max_difference(&presented) <= 1,
        "the copy into an encoded supplied texture changed the pixels by {}",
        composed.max_difference(&presented)
    );
}

#[test]
fn a_target_the_supplied_textures_cannot_match_stops_the_frame() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    let Some((mut renderer, textures)) = supplied(&graphics, 2) else {
        return;
    };

    assert!(renderer.present_into(0));
    present(&mut renderer, &filled(opaque(255, 0, 0)));
    // A display's mode holds still while a program runs, so this is a caller asking for something
    // the renderer cannot do: it did not create these textures and cannot create another. Drawing
    // anyway would copy a composed target of one extent across a buffer of another.
    renderer.configure(RenderTarget::new(
        Size::new(SIDE / 2, SIDE / 2),
        Scale::new(1.0),
    ));

    assert!(renderer.present_into(1));
    assert_eq!(
        renderer.draw(&filled(opaque(0, 0, 255)), &DamageSet::full()),
        FrameOutcome::Skipped(SkipReason::Unconfigured),
        "a frame was drawn into buffers of the wrong extent"
    );
    assert_eq!(
        centre(&read(&renderer, &textures[1])),
        [0, 0, 0, 0],
        "the skipped frame reached a buffer anyway"
    );
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
    renderer.configure(RenderTarget::new(
        Size::new(SIDE / 2, SIDE / 2),
        Scale::new(1.0),
    ));
    // Back to the extent the buffers have, which is the other way a caller ends the disagreement.
    renderer.configure(target());

    // The set is still the caller's own. A renderer that had quietly allocated textures of its own
    // would draw a perfectly good frame into them and leave these two untouched.
    assert!(renderer.present_into(1));
    present(&mut renderer, &filled(opaque(0, 0, 255)));
    assert_eq!(
        centre(&read(&renderer, &textures[1])),
        [0, 0, 255, 255],
        "the frame went somewhere other than the buffer the caller supplied"
    );
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
