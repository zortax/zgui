//! A video frame through the whole frame loop, on a real device, read back from the window.
//!
//! The producer presents I420 planes. The host converts them during the embed step, the
//! compositor draws the result into the window, and the composed pixels show the colours the
//! samples mean.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

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

/// An 8×2 picture of `left` codes on the left half and `right` codes on the right.
fn frame(gpu: &GpuShare, left: [u8; 3], right: [u8; 3]) -> VideoFrame {
    let luma: Vec<u8> = (0..16)
        .map(|i| if i % 8 < 4 { left[0] } else { right[0] })
        .collect();
    let cb = [left[1], left[1], right[1], right[1]];
    let cr = [left[2], left[2], right[2], right[2]];
    fn plane(bytes: &[u8], width: u32, height: u32) -> PlaneData<'_> {
        PlaneData {
            bytes,
            stride: width,
            width,
            height,
        }
    }
    let planes = Planes::upload_triplanar(
        gpu.device(),
        gpu.queue(),
        SampleSize::U8,
        [plane(&luma, 8, 2), plane(&cb, 4, 1), plane(&cr, 4, 1)],
    )
    .expect("8-bit planes are always supported");
    VideoFrame::new(planes, ColorSpace::BT709)
}

/// A window with one surface, its producer's handle, the device it attached to, and the
/// pixels the window last drew.
struct Fixture {
    app: Harness<Runtime>,
    handle: SurfaceHandle,
    gpu: GpuShare,
    last: Rc<RefCell<Option<Pixels>>>,
}

fn fixture() -> Option<Fixture> {
    let handle = SurfaceHandle::new(SurfaceConfig::default());
    let (tx, events) = std::sync::mpsc::channel();
    handle.set_events(move |event| {
        let _ = tx.send(event);
    });
    let last = Rc::new(RefCell::new(None));
    let mut app = app(handle.clone(), Rc::clone(&last))?;
    app.app_mut().windows_mut()[0].install_embed_host(Box::new(zgui_wgpu::WgpuSurfaces::new()));
    app.settle(16);
    let gpu = attached(&events);
    Some(Fixture {
        app,
        handle,
        gpu,
        last,
    })
}

impl Fixture {
    /// The left and right halves of the window as last drawn.
    fn halves(&self) -> ([u8; 4], [u8; 4]) {
        let last = self.last.borrow();
        let pixels = last.as_ref().expect("a frame was drawn");
        (pixels.rgba(10, 10), pixels.rgba(70, 10))
    }
}

fn near(actual: [u8; 4], expected: [u8; 3]) -> bool {
    actual[..3]
        .iter()
        .zip(expected)
        .all(|(&a, e)| a.abs_diff(e) <= 2)
}

#[test]
fn a_presented_video_frame_reaches_the_window_in_colour() {
    let Some(mut fx) = fixture() else { return };
    fx.handle.present_video(frame(&fx.gpu, RED, BLUE));
    fx.app.settle(16);
    let (left, right) = fx.halves();
    assert!(near(left, [255, 0, 0]), "left half: {left:?}");
    assert!(near(right, [0, 0, 255]), "right half: {right:?}");
}

#[test]
fn stamped_frames_show_on_the_refresh_nearest_their_stamps_and_wake_the_loop_for_it() {
    let Some(mut fx) = fixture() else { return };
    let start = fx.app.now();
    let at = |ms| start + Duration::from_millis(ms);
    fx.handle
        .present_video(frame(&fx.gpu, RED, RED).with_presentation_time(at(100)));
    fx.handle
        .present_video(frame(&fx.gpu, BLUE, BLUE).with_presentation_time(at(200)));
    fx.app.settle(16);
    assert!(
        fx.last
            .borrow()
            .as_ref()
            .is_none_or(|p| !near(p.rgba(10, 10), [255, 0, 0])),
        "nothing shows before its time"
    );

    // The loop parks until half a refresh before the first stamp, and no earlier.
    let parked = fx.app.parked_deadline().expect("a wake is owed");
    assert!(parked > at(80) && parked <= at(100), "{:?}", parked - start);

    fx.app.advance(Duration::from_millis(95));
    fx.app.settle(16);
    assert!(near(fx.halves().0, [255, 0, 0]), "{:?}", fx.halves().0);

    fx.app.advance(Duration::from_millis(100));
    fx.app.settle(16);
    assert!(near(fx.halves().0, [0, 0, 255]), "{:?}", fx.halves().0);
    // An empty queue wakes nothing: the next wake is the runtime's own maintenance, seconds out.
    let now = fx.app.now();
    assert!(
        fx.app
            .parked_deadline()
            .is_none_or(|d| d >= now + Duration::from_secs(1)),
        "the loop keeps waking for a queue that is empty"
    );
}

#[test]
fn an_unstamped_frame_empties_the_queue() {
    let Some(mut fx) = fixture() else { return };
    let start = fx.app.now();
    fx.handle.present_video(
        frame(&fx.gpu, BLUE, BLUE).with_presentation_time(start + Duration::from_millis(50)),
    );
    fx.handle.present_video(frame(&fx.gpu, RED, RED));
    fx.app.settle(16);
    fx.app.advance(Duration::from_millis(100));
    fx.app.settle(16);
    assert!(near(fx.halves().0, [255, 0, 0]), "{:?}", fx.halves().0);
}
