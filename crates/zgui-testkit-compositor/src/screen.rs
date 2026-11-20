//! A recording renderer whose last frame is the whole picture on the output.

use std::sync::{Arc, Mutex};

use compositor_api::Renderer;
use compositor_capture::CaptureRenderer;
use compositor_types::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget,
};

use crate::drawing::Drawing;

/// A renderer that records every frame and keeps no target between frames.
///
/// The compositor draws every frame it builds over the whole output, so the last frame is what the
/// output shows. A frame the compositor does not build changes nothing on the output.
#[derive(Debug, Default)]
pub struct Screen {
    capture: CaptureRenderer,
    shown: Shown,
}

/// What a [`Screen`]'s output shows, readable after the screen has been handed away.
#[derive(Clone, Debug, Default)]
pub struct Shown(Arc<Mutex<Option<Drawing>>>);

impl Shown {
    /// Replaces what the output shows.
    pub(crate) fn set(&self, drawing: Drawing) {
        *self.0.lock().unwrap_or_else(|held| held.into_inner()) = Some(drawing);
    }

    /// Returns what the output shows, or `None` before the first frame.
    pub fn drawing(&self) -> Option<Drawing> {
        self.0
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone()
    }
}

impl Screen {
    /// Returns a renderer with the capabilities of `RenderCapabilities::MINIMAL` that has drawn
    /// nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the renderer showing its frames through `shown`.
    #[must_use]
    pub fn showing_into(mut self, shown: Shown) -> Self {
        self.shown = shown;
        self
    }

    /// Returns what the output shows, or `None` before the first frame.
    pub fn drawing(&self) -> Option<Drawing> {
        self.shown.drawing()
    }

    /// Returns a handle that reads what the output shows.
    pub fn shown(&self) -> Shown {
        self.shown.clone()
    }

    /// Returns how many frames the renderer drew.
    pub fn frames(&self) -> u64 {
        self.capture.frames()
    }
}

impl Renderer for Screen {
    fn capabilities(&self) -> RenderCapabilities {
        self.capture.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.capture.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.capture.target()
    }

    fn target_age(&self) -> BufferAge {
        BufferAge::Unknown
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        let outcome = self.capture.draw(frame);
        if let Some(captured) = self.capture.last() {
            self.shown.set(Drawing::of(captured.drawing.clone()));
        }
        outcome
    }

    fn memory(&self) -> MemoryReport {
        self.capture.memory()
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

/// Returns what the output of `renderer` shows, when `renderer` is a [`Screen`] that has drawn.
pub fn drawing_of(renderer: &dyn Renderer) -> Option<Drawing> {
    renderer
        .as_any()?
        .downcast_ref::<Screen>()
        .and_then(Screen::drawing)
}
