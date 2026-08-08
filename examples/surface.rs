//! Surfaces: elements whose pixels another renderer produces.
//!
//! Run it with `cargo run -p zgui-examples --example surface --release`.
//!
//! What it is worth reading for:
//!
//! * `surface()` is an ordinary element — a box, ordinary CSS, ordinary layout — whose content is
//!   a texture on zgui's own graphics device;
//! * the [`SurfaceRenderer`] form is for content zgui drives: it owns no texture and no cadence,
//!   only the drawing, and asks for the next refresh with `request_animation_frame` — stop asking
//!   and it stops being called;
//! * the [`SurfaceHandle`] form is for a producer with a cadence of its own — here a thread
//!   playing "video" frames — which creates textures on the shared device, writes into them, and
//!   `present`s: latest wins, and the wake reaches the frame loop by itself;
//! * both scale, clip, sort and move exactly like any other box, because to the compositor they
//!   are one more quad.

use zgui::prelude::*;
use zgui::surface::{
    self as gpu, SurfaceConfig, SurfaceEvent, SurfaceIntrinsic, SurfaceRenderCx, wgpu,
};

/// Content zgui drives: a pane that breathes through the hue wheel, one clear per refresh.
struct Breathing;

impl SurfaceRenderer for Breathing {
    fn render(&mut self, cx: &mut SurfaceRenderCx<'_>) {
        let time = cx.timestamp.since_origin().as_secs_f64();
        let (r, g, b) = hue(time * 0.25);
        let mut encoder = cx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("breathing"),
            });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("breathing.clear"),
            multiview_mask: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cx.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        cx.queue.submit([encoder.finish()]);
        cx.request_animation_frame();
    }
}

/// A producer with its own cadence: a thread writing gradient "video" frames at thirty a second.
fn play_video(handle: gpu::SurfaceHandle) {
    handle.set_intrinsic(SurfaceIntrinsic::ratio(16.0 / 9.0));
    let events = std::sync::mpsc::channel();
    let sink = events.0;
    handle.set_events(move |event| {
        let _ = sink.send(event);
    });

    std::thread::spawn(move || {
        let receiver = events.1;
        // Wait for the element to be laid out and the device to be shared.
        let (gpu, mut size) = loop {
            match receiver.recv() {
                Ok(SurfaceEvent::Attached { gpu, size, .. }) => break (gpu, size),
                Ok(_) => continue,
                Err(_) => return,
            }
        };

        let mut frame = 0u64;
        let mut visible = true;
        loop {
            // The producer's own event handling: resize reallocates lazily, visibility pauses.
            while let Ok(event) = receiver.try_recv() {
                match event {
                    SurfaceEvent::Resized { size: new, .. } => size = new,
                    SurfaceEvent::Visible(now) => visible = now,
                    SurfaceEvent::Detached => return,
                    _ => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(33));
            if !visible || size.width == 0 || size.height == 0 {
                continue;
            }

            // A fresh texture per frame keeps the example honest about ownership; a real player
            // would round-robin three.
            let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("video.frame"),
                size: wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            gpu.queue().write_texture(
                texture.as_image_copy(),
                &gradient_frame(size.width, size.height, frame),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size.width * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth_or_array_layers: 1,
                },
            );
            handle.present(std::sync::Arc::new(texture));
            frame += 1;
        }
    });
}

/// One frame of the fake video: a gradient sliding sideways.
fn gradient_frame(width: u32, height: u32, frame: u64) -> Vec<u8> {
    let mut texels = Vec::with_capacity((width * height * 4) as usize);
    let shift = (frame * 3) as u32;
    for y in 0..height {
        for x in 0..width {
            let across = ((x + shift) % width.max(1)) as f32 / width.max(1) as f32;
            let down = y as f32 / height.max(1) as f32;
            texels.extend([(across * 255.0) as u8, (down * 160.0) as u8, 180, 255]);
        }
    }
    texels
}

/// A colour walking the hue wheel.
fn hue(turns: f64) -> (f64, f64, f64) {
    let phase = turns.fract() * std::f64::consts::TAU;
    let channel = |offset: f64| 0.5 + 0.5 * (phase + offset).sin();
    (
        channel(0.0),
        channel(std::f64::consts::TAU / 3.0),
        channel(2.0 * std::f64::consts::TAU / 3.0),
    )
}

/// The two surfaces side by side.
#[component]
fn Panel() -> impl IntoView {
    let video = gpu::SurfaceHandle::new(SurfaceConfig::default());
    play_video(video.clone());

    view! {
        column(class = "panel") {
            label(class = "panel__title") {"Surfaces"}
            row(class = "panel__row") {
                column(class = "cell") {
                    {zgui::elements::surface().class("pane").renderer(Breathing).into_view()}
                    label(class = "caption") {"driven by zgui"}
                }
                column(class = "cell") {
                    {zgui::elements::surface().class("pane").source(&video).into_view()}
                    label(class = "caption") {"a producer thread"}
                }
            }
        }
    }
}

/// How it looks.
const SHEET: &str = css!(
    ":root {
        background-color: #12141a;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .panel {
        align-items: center;
        gap: 20px;
        padding: 28px 36px;
        border-radius: 16px;
        border: 1px solid #262b36;
        background-color: #191d26;
    }

    .panel__title { font-size: 13px; letter-spacing: 2px; color: #7d879b; }
    .panel__row { gap: 28px; }
    .cell { align-items: center; gap: 10px; }

    .pane {
        width: 220px;
        height: 130px;
        border-radius: 10px;
        overflow: hidden;
        border: 1px solid #2c3342;
    }

    .caption { font-size: 12px; color: #7d879b; }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Surface")
        .with_title("Surfaces")
        .with_size(640.0, 360.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Panel() })
}
