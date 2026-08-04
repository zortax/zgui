//! The colour-encoding decision, measured rather than argued.
//!
//! Everything composites on premultiplied, gamma-encoded values. An `*Srgb` attachment format is a
//! fixed-function decode before every blend and an encode after it, so an encoded attachment moves
//! every blend into linear light however the shaders are written — and nothing above the renderer
//! can see it happen.
//!
//! `rgba(128, 128, 128, 0.5)` over white is the sharpest case. An eight-bit target holds
//! `128 / 255 = 0.50196` for both the channel and the alpha, so the premultiplied source is
//! `0.25196` at `0.50196` alpha and source-over gives `0.75` — **191**, which is what CSS
//! specifies. The same blend against an encoded attachment decodes the white destination to 1.0,
//! blends in linear light and re-encodes: **225**. Both figures are asserted below, on a real
//! device, so that the format choice is a measurement and not a belief.

mod support;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, Size};
use zgui_render_wgpu::bind::globals::{Globals, SubpixelOrder};
use zgui_render_wgpu::pipeline::Pipelines;
use zgui_render_wgpu::pipeline::kind::PipelineKind;
use zgui_render_wgpu::renderer::frame::FrameBuffers;
use zgui_render_wgpu::renderer::readback;
use zgui_render_wgpu::{Gpu, SrgbTier, wgpu};
use zgui_scene::{Quad, Scene};

use support::{SIDE, plain_renderer, present, rect, renderer};

/// The scene both halves of the measurement draw: opaque white, then half-transparent grey over it.
fn half_grey_over_white() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
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

#[test]
fn half_transparent_grey_over_white_reads_the_answer_css_specifies() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let scene = half_grey_over_white();
    let pixels = present(&mut renderer, &scene);
    assert_eq!(
        pixels.rgba(SIDE / 2, SIDE / 2),
        [191, 191, 191, 255],
        "an unencoded target blends where CSS says it does"
    );
}

#[test]
fn the_same_blend_against_an_encoded_attachment_is_thirty_four_levels_out() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let scene = half_grey_over_white();
    let plain = compose_into(renderer.gpu(), &scene, wgpu::TextureFormat::Rgba8Unorm);
    let encoded = compose_into(renderer.gpu(), &scene, wgpu::TextureFormat::Rgba8UnormSrgb);

    assert_eq!(
        plain.rgba(SIDE / 2, SIDE / 2)[0],
        191,
        "the unencoded attachment is the reference"
    );
    assert_eq!(
        encoded.rgba(SIDE / 2, SIDE / 2)[0],
        225,
        "the encoded attachment blends in linear light, which is the error being prevented"
    );
}

#[test]
fn an_encoded_surface_still_presents_the_bytes_that_were_composed() {
    // Both fallbacks for a surface that offers nothing unencoded. The first views the surface
    // under its unencoded twin; the second cancels the encode in the copy. Either way the
    // presented bytes have to be the composed ones, and the composed ones have to be the answer
    // the unencoded case gives.
    for (mutable, tier) in [
        (true, SrgbTier::ViewFormatTwin),
        (false, SrgbTier::UndoInBlit),
    ] {
        let Some(mut renderer) = renderer(wgpu::TextureFormat::Rgba8UnormSrgb, mutable) else {
            return;
        };
        assert_eq!(renderer.formats().tier, tier);
        assert_eq!(
            renderer.formats().scene,
            wgpu::TextureFormat::Rgba8Unorm,
            "the composed target never encodes"
        );

        let scene = half_grey_over_white();
        let presented = present(&mut renderer, &scene);
        let composed = renderer.read_composed();

        assert_eq!(
            composed.rgba(SIDE / 2, SIDE / 2),
            [191, 191, 191, 255],
            "{tier:?}: the blend still happens unencoded"
        );
        assert!(
            composed.max_difference(&presented) <= 1,
            "{tier:?}: the copy to an encoded surface changed the pixels by {}",
            composed.max_difference(&presented)
        );
    }
}

#[test]
fn the_composed_target_is_the_surface_format_with_its_encoding_removed() {
    for (surface, composed) in [
        (
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8Unorm,
        ),
        (
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
        ),
        (
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Rgba8Unorm,
        ),
    ] {
        let Some(renderer) = renderer(surface, true) else {
            return;
        };
        let formats = renderer.formats();
        assert_eq!(formats.surface, surface);
        assert_eq!(formats.scene, composed);
        assert!(formats.is_sound());
    }
}

/// Composes `scene` into an attachment of `format` and reads it back.
///
/// It goes through the same pipelines and the same instance data the renderer does, and differs
/// only in the attachment's format — which is the whole variable under test. The renderer itself
/// refuses to compose into an encoded attachment, which is why the counterfactual is built here.
fn compose_into(
    gpu: &std::sync::Arc<Gpu>,
    scene: &Scene,
    format: wgpu::TextureFormat,
) -> zgui_render_wgpu::Pixels {
    let size: Size<i32, Device> = Size::new(SIDE, SIDE);
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("test.attachment"),
        size: wgpu::Extent3d {
            width: SIDE as u32,
            height: SIDE as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut pipelines = Pipelines::new(gpu);
    let mut buffers = FrameBuffers::new(gpu);
    buffers.prepare_tables(scene);
    buffers.begin_frame(gpu);
    let globals = buffers
        .globals
        .stage(&Globals::new(size, SubpixelOrder::default()));
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("test.compose"),
        });
    buffers.upload_frame(gpu, &mut encoder, scene);
    buffers.finish_uploads();
    let frame = buffers
        .frame_bind_group(gpu, pipelines.layouts())
        .expect("the block describing the target has just been uploaded");
    let instances = buffers
        .instance_bind_group(gpu, pipelines.layouts(), PipelineKind::Quad)
        .expect("the quad pipeline draws instances");

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("test.compose"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let pipeline = pipelines
            .get(gpu, PipelineKind::Quad, format)
            .expect("the quad pipeline exists for every attachment format");
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &frame, &[globals]);
        pass.set_bind_group(1, &instances, &[]);
        pass.draw(0..4, 0..scene.primitives.quads.len() as u32);
    }
    gpu.queue().submit([encoder.finish()]);
    buffers.recall_uploads();
    readback::read(gpu, &texture, format, size)
}
