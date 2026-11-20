//! A graphics device under the application, drawing to a texture instead of to a window.
//!
//! It is the production stack minus the compositor: the same builder, the same format rules, and
//! the same rasteriser selection an opened window goes through. Only the destination differs, and
//! it differs because a window's surface belongs to the compositor the instant it is presented and
//! cannot be read back.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use zgui::render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, WgpuRenderer, wgpu};

use crate::raster::script::{self, Recorded};

/// Serialises every fixture in this binary onto the device.
///
/// A program has one graphics device and one reactive runtime per thread; these fixtures are the
/// only thing that would ask for several of either at once.
pub fn device_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|held| held.into_inner())
}

/// Every frame of one run, in the order they were drawn.
pub type Log = Arc<Mutex<Vec<Recorded>>>;

/// zgui's own renderer drawing the display list of every frame a window ran, with each frame's
/// readbacks recorded beside it.
///
/// The window itself draws through a compositor onto a renderer that draws nowhere. The display
/// list it still builds holds the vector content the compositor does not draw yet, and this draws
/// it.
struct Recording {
    /// The renderer, opened at the first frame for the window's extent.
    renderer: RefCell<Option<WgpuRenderer>>,
    /// What each frame produced.
    log: Log,
    /// The page's own background, which a scissored repaint is replayed over.
    background: [u8; 4],
    /// The atlas uploads the renderer has not been sent yet.
    uploads: RefCell<zgui_testkit_scene::Uploads>,
}

impl zgui::runtime::FrameProbe for Recording {
    fn frame_ended(&self, window: &zgui::runtime::Window) {
        let scene = window.scene();
        let target = RenderTarget::new(scene.viewport(), window.scale());
        let mut held = self.renderer.borrow_mut();
        if let Some(renderer) = held
            .as_mut()
            .filter(|renderer| renderer.target() != Some(target))
        {
            Renderer::configure(renderer, target);
        }
        let renderer = held.get_or_insert_with(|| {
            let mut renderer = Builder::new()
                .offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)
                .expect("a device opens, which `available` answered beforehand");
            // The same call a window makes, rather than a second copy of it: a fixture that wired
            // its own rasteriser up would stay green through the wiring being deleted.
            zgui_render_vector_vello::attach(&mut renderer, target.size);
            assert!(
                renderer.has_vector_raster(),
                "the selection handed back no rasteriser, so nothing below would draw a path"
            );
            renderer
        });
        self.uploads
            .borrow_mut()
            .replay(Renderer::texture_sink(renderer))
            .expect("the device takes the atlas uploads");
        let recorded = script::record(renderer, scene, self.background);
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

/// Returns `app` drawing every frame's display list with zgui's own renderer into `log`, over a
/// window that draws through a compositor onto a renderer that draws nowhere.
pub fn drawn(app: zgui::App, log: &Log, background: [u8; 4]) -> zgui::App {
    app.with_compositor_renderer(Box::new(|_size| {
        Box::new(zgui_testkit_compositor::NullRenderer::default())
    }))
    .with_probe(std::rc::Rc::new(Recording {
        renderer: RefCell::new(None),
        log: Arc::clone(log),
        background,
        uploads: RefCell::default(),
    }))
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
