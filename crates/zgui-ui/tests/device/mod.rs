//! A graphics device under the gallery, drawing to a texture instead of to a window.
//!
//! A fixture draws through one of two pipelines, picked with a [`Pipeline`]. The compositor draws
//! boxes and text onto its reference renderer, see [`compositor`]. zgui's own renderer draws the
//! display list's vector content and images, see [`recording`], with the same builder, format
//! rules and rasteriser selection an opened window went through. Only the destination differs,
//! because a window's surface belongs to the platform's compositor the instant it is presented and
//! cannot be read back.

#![allow(
    dead_code,
    unreachable_pub,
    reason = "one support module serves several groups of assertions, none of which uses all of it"
)]

pub mod compositor;
pub mod frame;
pub mod ink;
pub mod picture;
pub mod shot;

use std::cell::RefCell;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use zgui::render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, WgpuRenderer, wgpu};

use crate::device::frame::Frame;

/// Serialises every fixture in this binary onto the device.
///
/// A process has one graphics device and one reactive runtime per thread; these fixtures are the
/// only thing that would ask for several of either at once.
pub fn device_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|held| held.into_inner())
}

/// Every frame of one run, in the order they were drawn.
pub type Log = Arc<Mutex<Vec<Frame>>>;

/// Whether frames are drawn with the damage the application asked for.
static REAL_DAMAGE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Draws every frame with the application's own damage set from now on.
///
/// The default redraws the whole window every frame, which answers what the document *contains*.
/// A fixture asking what a partial repaint left on the screen wants the real damage, and reads
/// the composed target rather than the presented one.
#[allow(dead_code, reason = "not every fixture asks for it")]
pub fn use_real_damage() {
    REAL_DAMAGE.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// How many frames moved composed pixels in place of drawing a scrolled port.
pub(crate) static SHIFTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many frames so far answered a scroll by moving composed pixels.
#[allow(dead_code, reason = "not every fixture scrolls")]
pub fn shifts() -> usize {
    SHIFTS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Reports whether frames are drawn with the application's own damage set.
pub fn real_damage() -> bool {
    REAL_DAMAGE.load(std::sync::atomic::Ordering::Relaxed)
}

/// zgui's own renderer drawing the display list of every frame a window ran, with each frame
/// recorded beside it.
///
/// The window itself draws through a compositor onto a renderer that draws nowhere. The display
/// list it still builds holds the vector content, images and surfaces the compositor does not draw
/// yet, and this draws it.
struct Recording {
    /// The renderer, opened at the first frame for the window's extent.
    renderer: RefCell<Option<WgpuRenderer>>,
    /// The atlas uploads the renderer has not been sent yet.
    uploads: RefCell<zgui_testkit_scene::Uploads>,
    /// What each frame produced.
    log: Log,
}

impl Recording {
    /// Returns the renderer for a window at `viewport` and `scale`, opened or reconfigured as needed.
    fn renderer(
        held: &mut Option<WgpuRenderer>,
        viewport: zgui::geom::Size<i32, zgui::geom::Device>,
        scale: f32,
    ) -> &mut WgpuRenderer {
        let target = RenderTarget::new(viewport, zgui::geom::Scale::new(scale));
        if let Some(renderer) = held
            .as_mut()
            .filter(|renderer| renderer.target() != Some(target))
        {
            renderer.configure(target);
        }
        held.get_or_insert_with(|| {
            let mut renderer = Builder::new()
                .offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)
                .expect("a device opens, which `available` answered beforehand");
            // The same call a window makes, rather than a second copy of it.
            zgui_render_vector_vello::attach(&mut renderer, target.size);
            assert!(
                renderer.has_vector_raster(),
                "the selection handed back no rasteriser, so nothing below would draw a path"
            );
            renderer
        })
    }
}

impl zgui::runtime::FrameProbe for Recording {
    fn frame_ended(&self, window: &zgui::runtime::Window) {
        let scene = window.scene();
        let mut held = self.renderer.borrow_mut();
        let renderer = Self::renderer(&mut held, scene.viewport(), window.scale().get());
        self.uploads
            .borrow_mut()
            .replay(Renderer::texture_sink(renderer))
            .expect("the device takes the atlas uploads");
        // Over the whole surface, whatever the application asked for. The question is what the
        // gallery *contains*, and a scissored frame answers it only for the rectangle it redrew.
        // The real damage set when a fixture asks for it: the composed target then retains what
        // earlier frames drew, which is what a surface shows.
        let damage = match window.last_frame_report() {
            Some(report) if real_damage() => report.painted,
            _ => zgui::bits::DamageSet::full(),
        };
        let outcome = renderer.draw(scene, &damage);
        let recorded = Frame::record(renderer, scene, &outcome);
        self.log
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push(recorded);
    }

    fn atlas_uploads(&self) -> Option<std::cell::RefMut<'_, dyn zgui::atlas::TextureSink>> {
        Some(std::cell::RefMut::map(
            self.uploads.borrow_mut(),
            |uploads| uploads as &mut dyn zgui::atlas::TextureSink,
        ))
    }
}

/// Builds the probe that draws every frame's display list with zgui's own renderer into `log`.
pub fn recording(log: &Log) -> std::rc::Rc<dyn zgui::runtime::FrameProbe> {
    std::rc::Rc::new(Recording {
        renderer: RefCell::new(None),
        uploads: RefCell::default(),
        log: Arc::clone(log),
    })
}

/// The renderer a window under test draws through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pipeline {
    /// The compositor, which draws boxes and text.
    Compositor,
    /// zgui's own renderer, which draws the display list's vector content, images and groups.
    Scene,
}

/// Returns `app` drawing every window through `pipeline`, with each frame written into `log`.
pub fn drawn_on(app: zgui::App, pipeline: Pipeline, log: &Log) -> zgui::App {
    match pipeline {
        Pipeline::Compositor => app.with_compositor_renderer(compositor::factory(log)),
        Pipeline::Scene => app
            .with_compositor_renderer(Box::new(|_size| {
                Box::new(zgui_testkit_compositor::NullRenderer::default())
            }))
            .with_probe(recording(log)),
    }
}

/// Reports whether this machine has a graphics device `pipeline` runs on.
pub fn available_on(pipeline: Pipeline) -> bool {
    match pipeline {
        Pipeline::Compositor => compositor::available(),
        Pipeline::Scene => available(),
    }
}

/// Whether this machine has a graphics device these fixtures can run on at all.
pub fn available() -> bool {
    static ANSWER: OnceLock<bool> = OnceLock::new();
    *ANSWER.get_or_init(|| {
        let target = RenderTarget::new(zgui::geom::Size::new(64, 64), zgui::geom::Scale::new(1.0));
        match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
            Ok(_) => true,
            Err(failure) => {
                eprintln!("skipped: no usable graphics device ({failure})");
                false
            }
        }
    })
}
