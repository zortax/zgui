//! The texture zgui allocates for an on-demand renderer carries the usages the renderer asks for.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use compositor_wgpu::{Builder, WgpuRenderer};
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_runtime::{App, AppError, Runtime};
use zgui_view::{BuildCx, IntoView, View};
use zgui_wgpu::{SurfaceElementExt, SurfaceRenderCx, SurfaceRenderer, WgpuSurfaces, wgpu};

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

/// A window drawn by the compositor's wgpu renderer into an offscreen texture.
fn app(
    renderer: impl FnOnce() -> WgpuRenderer + Send + 'static,
    surface: ReadBack,
) -> Harness<Runtime> {
    let mut renderer = Some(renderer);
    let mut surface = Some(surface);
    let handler = App::new()
        .with_title("usage")
        .with_size(64.0, 64.0)
        .with_stylesheet(
            "root { display: block; width: 64px; height: 64px } surface { width: 32px; height: 32px }",
        )
        .with_compositor(zgui_testkit_compositor::driver(
            zgui_testkit_compositor::fake_text_compositor(),
        ))
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target| -> Result<_, AppError> {
                let make = renderer.take().expect("one window asks for one renderer");
                Ok(zgui_testkit_compositor::factory(move || {
                    let mut renderer = make();
                    compositor_api::Renderer::configure(&mut renderer, target);
                    renderer
                }))
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit::MonoShaper::new(),
            ))
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
    let open = || Builder::new().offscreen([64, 64], wgpu::TextureFormat::Bgra8Unorm);
    if let Err(failure) = open() {
        eprintln!("skipped: no usable graphics device ({failure})");
        return;
    }
    let read = Arc::new(Mutex::new(None));
    let mut app = app(
        move || open().expect("a device opens, which the test answered beforehand"),
        ReadBack {
            read: Arc::clone(&read),
        },
    );
    app.run_for(Duration::from_millis(50), Duration::from_millis(16));
    assert_eq!(*read.lock().unwrap(), Some(CLEARED));
}
