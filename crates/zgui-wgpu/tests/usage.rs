//! The texture zgui allocates for an on-demand renderer carries the usages the renderer asks for.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use zgui_geom::{Scale, Size};
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_render::RenderTarget;
use zgui_render_wgpu::{Builder, WgpuRenderer, wgpu};
use zgui_runtime::{App, AppError, Runtime};
use zgui_view::{BuildCx, IntoView, View};
use zgui_wgpu::{SurfaceElementExt, SurfaceRenderCx, SurfaceRenderer, WgpuSurfaces};

/// The colour the renderer clears to, as stored bytes.
const CLEARED: [u8; 4] = [255, 128, 0, 255];

/// Clears its texture, then copies one texel of it to a buffer and reads it.
struct ReadBack {
    /// The texel read, once a render ran.
    read: Arc<Mutex<Option<[u8; 4]>>>,
}

impl SurfaceRenderer for ReadBack {
    fn usage(&self) -> wgpu::TextureUsages {
        wgpu::TextureUsages::COPY_SRC
    }

    fn render(&mut self, cx: &mut SurfaceRenderCx<'_>) {
        let buffer = cx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("test.readback"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = cx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cx.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 1.0,
                        g: 128.0 / 255.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        encoder.copy_texture_to_buffer(
            cx.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout::default(),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        cx.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        cx.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the copy completes");
        let view = buffer
            .slice(..)
            .get_mapped_range()
            .expect("the buffer is mapped");
        let texel: [u8; 4] = view[..].try_into().expect("four bytes were copied");
        *self.read.lock().unwrap() = Some(texel);
    }
}

/// A window drawn by a real wgpu renderer into an offscreen texture.
fn app(renderer: WgpuRenderer, surface: ReadBack) -> Harness<Runtime> {
    let mut renderer = Some(renderer);
    let mut surface = Some(surface);
    let handler = App::new()
        .with_title("usage")
        .with_size(64.0, 64.0)
        .with_stylesheet(
            "root { display: block; width: 64px; height: 64px } surface { width: 32px; height: 32px }",
        )
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target| -> Result<_, AppError> {
                let mut renderer = renderer.take().expect("one window asks for one renderer");
                zgui_render::Renderer::configure(&mut renderer, target);
                Ok(Box::new(renderer) as Box<dyn zgui_render::Renderer>)
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| {
            Arc::new(zgui_testkit_scene::MonoRaster::new())
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let surface = surface.take().expect("the view is built once");
            Box::new(
                zgui_elements::r#box()
                    .class("root")
                    .child(zgui_elements::surface().renderer(surface))
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.app_mut().windows_mut()[0].install_embed_host(Box::new(WgpuSurfaces::new()));
    harness
}

#[test]
fn a_renderer_asking_for_copy_src_can_copy_its_texture_to_a_buffer() {
    let target = RenderTarget::new(Size::new(64, 64), Scale::new(1.0));
    let renderer = match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
        Ok(renderer) => renderer,
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            return;
        }
    };
    let read = Arc::new(Mutex::new(None));
    let mut app = app(
        renderer,
        ReadBack {
            read: Arc::clone(&read),
        },
    );
    app.run_for(Duration::from_millis(50), Duration::from_millis(16));
    assert_eq!(*read.lock().unwrap(), Some(CLEARED));
}
