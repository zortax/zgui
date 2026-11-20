//! A surface element whose content box changes size is drawn from a new external texture slot.
//!
//! A slot holds the extent it was created with, so a resized element gets a new one. The old slot
//! is still named by the chunk the compositor holds until the frame records the element again,
//! and a commit that removed it any earlier would be refused. A refused commit registers the
//! window as a new client, which every external quad then names.

use std::sync::Arc;

use zgui_geom::{DevicePx, Size};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_platform_headless::Harness;
use zgui_runtime::{App, AppError, Runtime};
use zgui_testkit_compositor::{Device, Shown};
use zgui_view::{BuildCx, IntoView, View};
use zgui_wgpu::{SurfaceElementExt, SurfaceRenderCx, SurfaceRenderer, WgpuSurfaces, wgpu};

/// Clears its texture to one colour.
struct Clear;

impl SurfaceRenderer for Clear {
    fn render(&mut self, cx: &mut SurfaceRenderCx<'_>) {
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
                    load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        cx.queue.submit([encoder.finish()]);
    }
}

/// A window drawn on a device whose surface element is half the window wide, and what the device
/// shows.
///
/// Returns `None` when no device opens.
fn app() -> Option<(Harness<Runtime>, Shown)> {
    let device = match Device::open([200, 100]) {
        Ok(device) => device,
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            return None;
        }
    };
    let shown = device.shown();
    let mut device = Some(device);
    let handler = App::new()
        .with_title("resized")
        .with_size(200.0, 100.0)
        .with_stylesheet(
            "root { display: block; width: 100%; height: 100% } \
             surface { display: block; width: 50%; height: 40px }",
        )
        .with_compositor(zgui_testkit_compositor::fake_text_compositor())
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target| -> Result<_, AppError> {
                let mut device = device.take().expect("one window asks for one renderer");
                compositor_api::Renderer::configure(&mut device, target);
                Ok(Box::new(device) as Box<dyn compositor_api::Renderer>)
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .into_handler(|cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::r#box()
                    .class("root")
                    .child(zgui_elements::surface().renderer(Clear))
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.app_mut().windows_mut()[0].install_embed_host(Box::new(WgpuSurfaces::new()));
    Some((harness, shown))
}

/// Returns the texture field of the external quad the last frame drew, such as `client1#0.1`.
fn texture(shown: &Shown) -> String {
    let drawing = shown.drawing().expect("the device drew a frame");
    let externals: Vec<_> = drawing.objects("external").collect();
    let [external] = externals.as_slice() else {
        panic!("one external quad: {externals:?}");
    };
    external
        .field("texture")
        .expect("an external quad names its texture")
        .to_owned()
}

#[test]
fn a_resized_surface_is_drawn_from_a_new_slot_of_the_same_client() {
    let Some((mut app, shown)) = app() else {
        return;
    };
    app.settle(16);
    let first = texture(&shown);

    let surface = app
        .platform()
        .offscreens()
        .first()
        .map(|surface| Surface::id(surface.as_ref()))
        .expect("the application opened its window");
    app.deliver(
        surface,
        SurfaceEvent::Resized(Size::new(DevicePx(300.0), DevicePx(100.0))),
    );
    app.settle(16);
    let second = texture(&shown);

    let client = |texture: &str| texture.split('#').next().unwrap_or_default().to_owned();
    assert_ne!(first, second, "the wider element is drawn from a new slot");
    assert_eq!(
        client(&first),
        client(&second),
        "and no commit was refused on the way: {first} then {second}"
    );
}
