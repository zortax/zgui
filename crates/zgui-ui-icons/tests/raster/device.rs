//! A graphics device under the application, drawing to a texture instead of to a window.
//!
//! It is the production stack minus the window: the compositor the application draws through and
//! the reference renderer on a real device. Only the destination differs, and it differs because a
//! window's surface belongs to the display server the instant it is presented and cannot be read
//! back.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use compositor_types::{BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities};
use zgui_testkit_compositor::{Composed, Device, Shown};

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

/// The device the compositor draws with, with each frame's readback recorded beside it.
struct Recording {
    /// The device.
    device: Device,
    /// What the device's last frame drew.
    shown: Shown,
    /// The device's target.
    composed: Composed,
    /// What each frame produced.
    log: Log,
}

impl compositor_api::Renderer for Recording {
    fn capabilities(&self) -> RenderCapabilities {
        self.device.capabilities()
    }

    fn configure(&mut self, target: compositor_types::RenderTarget) {
        self.device.configure(target);
    }

    fn target(&self) -> Option<compositor_types::RenderTarget> {
        self.device.target()
    }

    fn target_age(&self) -> BufferAge {
        self.device.target_age()
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        let outcome = self.device.draw(frame);
        let drawing = self.shown.drawing().expect("the device recorded the frame");
        let pixels = self
            .composed
            .pixels()
            .expect("the device read its target back");
        self.log
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push(script::record(&drawing, pixels));
        outcome
    }

    fn memory(&self) -> MemoryReport {
        self.device.memory()
    }
}

/// Builds the renderer an application under test draws its compositor's frames with, writing
/// every frame into `log`.
pub fn factory(log: &Log) -> zgui::CompositorRendererMaker {
    let log = Arc::clone(log);
    Box::new(move |size: [u32; 2]| {
        let device = Device::open(size).expect("the device opened a moment ago");
        Box::new(Recording {
            shown: device.shown(),
            composed: device.composed(),
            device,
            log: Arc::clone(&log),
        }) as Box<dyn compositor_api::Renderer>
    })
}

/// Whether this machine has a graphics device these fixtures can run on at all.
pub fn available() -> bool {
    static ANSWER: OnceLock<bool> = OnceLock::new();
    *ANSWER.get_or_init(|| match Device::open([64, 64]) {
        Ok(_) => true,
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            false
        }
    })
}
