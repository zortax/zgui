//! A renderer that draws nothing.

use compositor_api::Renderer;
use compositor_types::{
    BufferAge, Frame, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
};

/// A renderer that accepts every frame, reports it presented and keeps nothing.
///
/// It reports the minimal capabilities, so every frame it is given carries full tables and no
/// resident chunks.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullRenderer {
    target: Option<RenderTarget>,
}

impl Renderer for NullRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn target_age(&self) -> BufferAge {
        BufferAge::Unknown
    }

    fn draw(&mut self, _frame: &Frame) -> FrameOutcome {
        FrameOutcome::Presented(FrameStats::default())
    }

    fn memory(&self) -> MemoryReport {
        MemoryReport::default()
    }
}
