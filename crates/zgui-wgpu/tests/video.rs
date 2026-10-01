//! A video frame through the whole frame loop, on a real device, read back from the window.
//!
//! The producer presents I420 planes. The host converts them during the embed step, the
//! compositor draws the result into the window, and the composed pixels show the colours the
//! samples mean.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui_render_wgpu::{Builder, Pixels, WgpuRenderer, wgpu};
use zgui_runtime::{App, AppError, Runtime};
use zgui_scene::Scene;
use zgui_view::{Anchor, BuildCx, IntoView, View};
use zgui_wgpu::{
    ColorSpace, GpuShare, PlaneData, Planes, SampleSize, SurfaceConfig, SurfaceElementExt,
    SurfaceEvent, SurfaceHandle, VideoFrame,
};

/// BT.709 limited-range samples of pure red and pure blue.
const RED: [u8; 3] = [63, 102, 240];
const BLUE: [u8; 3] = [32, 240, 118];

/// A wgpu renderer that keeps the pixels of the last frame it drew.
struct Recording {
    /// The renderer that draws.
    inner: WgpuRenderer,
    /// The composed target after the last draw.
    last: Rc<RefCell<Option<Pixels>>>,
}

impl Renderer for Recording {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let outcome = self.inner.draw(scene, damage);
        *self.last.borrow_mut() = Some(self.inner.read_composed());
        outcome
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn TextureSink {
        self.inner.texture_sink()
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(&mut self.inner)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(&self.inner)
    }
}

/// A window whose one surface element fills it, drawn by a real device.
fn app(handle: SurfaceHandle, last: Rc<RefCell<Option<Pixels>>>) -> Option<Harness<Runtime>> {
    let target = RenderTarget::new(zgui_geom::Size::new(80, 20), zgui_geom::Scale::new(1.0));
    if let Err(failure) = Builder::new().offscreen(target, wgpu::TextureFormat::Rgba8Unorm, false) {
        eprintln!("skipped: no usable graphics device ({failure})");
        return None;
    }
    let handler = App::new()
        .with_title("video")
        .with_size(80.0, 20.0)
        .with_stylesheet("root { display: block; width: 80px; height: 20px } surface { width: 80px; height: 20px }")
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target| -> Result<_, AppError> {
                let inner = Builder::new()
                    .offscreen(target, wgpu::TextureFormat::Rgba8Unorm, false)
                    ?;
                Ok(Box::new(Recording {
                    inner,
                    last: Rc::clone(&last),
                }) as Box<dyn Renderer>)
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
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(
                zgui_elements::r#box()
                    .class("root")
                    .child(zgui_elements::surface().source(&handle))
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    Some(Harness::new(handler))
}

/// The device the host attached the surface to.
fn attached(events: &Receiver<SurfaceEvent>) -> GpuShare {
    events
        .try_iter()
        .find_map(|event| match event {
            SurfaceEvent::Attached { gpu, .. } => Some(gpu),
            _ => None,
        })
        .expect("the surface was attached")
}

/// An 8×2 picture: red on the left half, blue on the right.
fn frame(gpu: &GpuShare) -> VideoFrame {
    let luma: Vec<u8> = (0..16)
        .map(|i| if i % 8 < 4 { RED[0] } else { BLUE[0] })
        .collect();
    let cb = [RED[1], RED[1], BLUE[1], BLUE[1]];
    let cr = [RED[2], RED[2], BLUE[2], BLUE[2]];
    let plane = |bytes: &'static [u8], width: u32, height: u32| PlaneData {
        bytes,
        stride: width,
        width,
        height,
    };
    let (luma, cb, cr) = (luma.leak(), cb.to_vec().leak(), cr.to_vec().leak());
    let planes = Planes::upload_triplanar(
        gpu.device(),
        gpu.queue(),
        SampleSize::U8,
        [plane(luma, 8, 2), plane(cb, 4, 1), plane(cr, 4, 1)],
    )
    .expect("8-bit planes are always supported");
    VideoFrame::new(planes, ColorSpace::BT709)
}

fn near(actual: [u8; 4], expected: [u8; 3]) -> bool {
    actual[..3]
        .iter()
        .zip(expected)
        .all(|(&a, e)| a.abs_diff(e) <= 2)
}

#[test]
fn a_presented_video_frame_reaches_the_window_in_colour() {
    let handle = SurfaceHandle::new(SurfaceConfig::default());
    let (tx, events) = std::sync::mpsc::channel();
    handle.set_events(move |event| {
        let _ = tx.send(event);
    });
    let last = Rc::new(RefCell::new(None));
    let Some(mut app) = app(handle.clone(), Rc::clone(&last)) else {
        return;
    };
    app.app_mut().windows_mut()[0].install_embed_host(Box::new(zgui_wgpu::WgpuSurfaces::new()));
    app.settle(16);

    let gpu = attached(&events);
    handle.present_video(frame(&gpu));
    app.settle(16);

    let pixels = last.borrow_mut().take().expect("a frame was drawn");
    let (left, right) = (pixels.rgba(10, 10), pixels.rgba(70, 10));
    assert!(near(left, [255, 0, 0]), "left half: {left:?}");
    assert!(near(right, [0, 0, 255]), "right half: {right:?}");
}
