//! A recording renderer that keeps its target between frames.

use std::sync::{Arc, Mutex};

use iceblink::{
    BufferAge, Frame, FrameDamage, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, Shift,
};
use iceblink_testing::capture::CaptureRenderer;

use crate::drawing::Drawing;

/// A renderer that records every frame and keeps the last one it presented.
///
/// Each frame after the first presented one is built against the kept target, so it redraws only
/// its damage, and a renderer that reports `can_shift` is given the shifts its compositor decides.
#[derive(Debug)]
pub struct Kept {
    capture: CaptureRenderer,
    frames: Frames,
}

/// The frames a [`Kept`] renderer drew, readable after the renderer has been handed away.
#[derive(Clone, Debug, Default)]
pub struct Frames(Arc<Mutex<Vec<Drawn>>>);

/// One frame a [`Kept`] renderer drew.
#[derive(Clone, Debug)]
pub struct Drawn {
    /// The frame's damage.
    pub damage: FrameDamage,
    /// The pixel moves the frame applied before it drew its damage.
    pub shifts: Vec<Shift>,
    /// What the frame redrew.
    pub drawing: Drawing,
}

impl Frames {
    /// Returns every frame drawn so far, oldest first.
    pub fn drawn(&self) -> Vec<Drawn> {
        self.0
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone()
    }

    /// Forgets the frames drawn so far.
    pub fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clear();
    }

    pub(crate) fn push(&self, drawn: Drawn) {
        self.0
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push(drawn);
    }
}

impl Kept {
    /// Returns a renderer that reports `caps` and has drawn nothing.
    pub fn new(caps: RenderCapabilities) -> Self {
        Self {
            capture: CaptureRenderer::with_capabilities(caps),
            frames: Frames::default(),
        }
    }

    /// Returns the renderer recording its frames into `frames`.
    #[must_use]
    pub fn recording_into(mut self, frames: Frames) -> Self {
        self.frames = frames;
        self
    }

    /// Returns a handle that reads the frames drawn.
    pub fn frames(&self) -> Frames {
        self.frames.clone()
    }
}

impl Renderer for Kept {
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
        self.capture.target_age()
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        let outcome = self.capture.draw(frame);
        if let Some(captured) = self.capture.last() {
            self.frames.push(Drawn {
                damage: frame.damage,
                shifts: frame.shifts.to_vec(),
                drawing: Drawing::of(captured.drawing.clone()),
            });
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
