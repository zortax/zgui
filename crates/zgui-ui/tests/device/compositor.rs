//! The compositor's reference renderer under the gallery, drawing to a texture.
//!
//! A window draws its boxes and text through the compositor. Vector content, images and groups are
//! not drawn on this path yet, so a fixture asking about those draws through
//! [`factory`](crate::device::factory) instead.

use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use compositor_api::Renderer;
use compositor_types::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget,
};
use zgui_testkit_compositor::Device;

use crate::device::{Log, SHIFTS, frame, real_damage};

/// The device, with each frame recorded beside it.
struct Recording {
    device: Device,
    log: Log,
}

impl Renderer for Recording {
    fn capabilities(&self) -> RenderCapabilities {
        self.device.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.device.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.device.target()
    }

    // An unknown age makes every frame the compositor builds cover the whole output, so each
    // transcript is the whole window. The real damage set keeps the device's own age.
    fn target_age(&self) -> BufferAge {
        if real_damage() {
            self.device.target_age()
        } else {
            BufferAge::Unknown
        }
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        let outcome = self.device.draw(frame);
        let drawing = self
            .device
            .shown()
            .drawing()
            .expect("the device recorded the frame it drew");
        let pixels = self
            .device
            .composed()
            .pixels()
            .expect("the device read back the frame it drew");
        if !frame.shifts.is_empty() {
            SHIFTS.fetch_add(1, Ordering::Relaxed);
        }
        self.log
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push(frame::Frame::composed(&drawing, &pixels));
        outcome
    }

    fn memory(&self) -> MemoryReport {
        self.device.memory()
    }
}

/// Builds the renderer the compositor of the application under test draws through, writing every
/// frame into `log`.
///
/// # Panics
///
/// The renderer panics when no device opens, which [`available`] answers beforehand.
pub fn factory(log: &Log) -> zgui::CompositorRendererMaker {
    let log = std::sync::Arc::clone(log);
    Box::new(move |size| {
        let device = Device::open(size).expect("a device opens for the compositor");
        Box::new(Recording {
            device,
            log: std::sync::Arc::clone(&log),
        }) as Box<dyn Renderer>
    })
}

/// Reports whether this machine has a device the compositor's renderer opens on.
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
