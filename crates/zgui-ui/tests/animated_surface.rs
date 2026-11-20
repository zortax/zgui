//! A surface that draws every frame shows one whole frame at a time.
//!
//! The renderer clears its texture to a new colour each frame. Every composed frame must show one
//! colour across the whole surface, also while other content repaints over part of it.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::surface::{SurfaceElementExt, SurfaceRenderCx, SurfaceRenderer, wgpu};
use zgui::view;
use zgui::view::AnyView;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff }
    .page { padding: 20px }
    .preview { width: 200px; height: 200px; border-radius: 8px; border: 1px solid #cccccc }
    .badge { position: absolute; left: 60px; top: 60px; width: 30px; height: 30px }";

/// Clears to a colour that steps every frame.
struct Stepping {
    frame: u32,
}

impl SurfaceRenderer for Stepping {
    fn render(&mut self, cx: &mut SurfaceRenderCx<'_>) {
        self.frame += 1;
        let level = f64::from(self.frame % 8) / 8.0;
        let mut encoder = cx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cx.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: level,
                        g: 0.0,
                        b: 1.0 - level,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        cx.queue.submit([encoder.finish()]);
        cx.request_animation_frame();
    }
}

fn scene(badge: RwSignal<bool>) -> impl Fn() -> AnyView {
    move || {
        let surface = zgui::elements::surface()
            .class("preview")
            .renderer(Stepping { frame: 0 });
        AnyView::new(view! {
            box(class = "page") {
                {surface.into_view()}
                box(class = "badge", style:background-color = move || {
                    Some(if badge.get() { "#00ff00" } else { "transparent" }.to_owned())
                }) {}
            }
        })
    }
}

#[test]
fn every_frame_shows_one_surface_frame() {
    run(1.0);
}

#[test]
fn every_frame_shows_one_surface_frame_at_a_fractional_scale() {
    run(1.2);
}

fn run(scale: f64) {
    crate::device::use_real_damage();
    let badge = RwSignal::new(false);
    let Some(mut stage) = Stage::open(SHEET, scene(badge)) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    if scale != 1.0 {
        stage.rescale(scale);
    }
    let s = scale as f32;
    // Inside the content box, clear of the rounded corners and of the badge.
    let inside = Rect::new(
        Point::new(DevicePx(30.0 * s), DevicePx(110.0 * s)),
        Size::<DevicePx, Device>::new(DevicePx(180.0 * s), DevicePx(100.0 * s)),
    );
    let top = Rect::new(
        Point::new(DevicePx(100.0 * s), DevicePx(30.0 * s)),
        Size::<DevicePx, Device>::new(DevicePx(110.0 * s), DevicePx(40.0 * s)),
    );
    let mut seen = Vec::new();
    for step in 0..24 {
        if step % 3 == 0 {
            badge.set(!badge.get_untracked());
        }
        stage.tick();
        let mut colours: Vec<(u8, u8, u8)> = stage.composed_colours_in(inside);
        colours.extend(stage.composed_colours_in(top));
        colours.sort_unstable();
        colours.dedup();
        if colours.len() > 1 {
            stage.capture_composed(&format!("surface-{step}"));
        }
        assert_eq!(
            colours.len(),
            1,
            "step {step}: the surface shows {colours:?}"
        );
        seen.push(colours[0]);
    }
    seen.dedup();
    assert!(seen.len() > 12, "the surface animated through {seen:?}");
}
