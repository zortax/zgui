//! A recording renderer whose last frame is the whole picture on the output.

use std::sync::{Arc, Mutex};

use iceblink::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
};
use iceblink_testing::capture::CaptureRenderer;

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
pub struct Shown(Arc<Mutex<Showing>>);

/// The last frame a [`Screen`] drew, and how many it drew.
#[derive(Debug, Default)]
struct Showing {
    drawing: Option<Drawing>,
    frames: u64,
}

impl Shown {
    /// Replaces what the output shows, with the frame drawn the `frames`th.
    pub(crate) fn set(&self, drawing: Drawing, frames: u64) {
        let mut showing = self.0.lock().unwrap_or_else(|held| held.into_inner());
        showing.drawing = Some(drawing);
        showing.frames = frames;
    }

    /// Returns what the output shows, or `None` before the first frame.
    pub fn drawing(&self) -> Option<Drawing> {
        self.0
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .drawing
            .clone()
    }

    /// Returns how many frames the screen drew.
    pub fn frames(&self) -> u64 {
        self.0
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .frames
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
            self.shown
                .set(Drawing::of(captured.drawing.clone()), self.capture.frames());
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
