//! The readbacks and transcripts of a window drawing on this machine's device.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use compositor_api::Renderer;
use compositor_capture::CaptureRenderer;
use compositor_types::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, VectorStatus,
};
use compositor_wgpu::{Pixels, WgpuRenderer};
use zgui::geom::{Device, Point, Rect, Size};

/// What one frame was drawn against: whether the whole surface, and the rectangles otherwise.
pub(crate) type Drawn = (bool, Vec<Rect<i32, Device>>);

/// The pixels a window last presented, and the drawings of its frames, kept for a comparison to
/// read.
///
/// The driver owns the window's renderer, so the only place a readback can be taken from is
/// inside the renderer itself, after the draw and before anything else can touch the target.
#[derive(Default)]
pub(crate) struct Capture {
    /// Whether the next frame should be read back, which is off by default because a readback is
    /// a full device synchronisation and a measurement must not pay for one.
    pub(crate) want: AtomicBool,
    /// What the last frame that was asked for presented.
    pub(crate) last: Mutex<Option<Pixels>>,
    /// The persistent target that frame was composed into, which is the one a partial repaint
    /// accumulates in and the one a stale pixel would survive in.
    pub(crate) composed: Mutex<Option<Pixels>>,
    /// The draw damage of every frame since the list was last emptied.
    pub(crate) damages: Mutex<Vec<Drawn>>,
    /// The drawing of every frame drawn over the whole output.
    pub(crate) full_lists: Mutex<Vec<String>>,
    /// Whether the drawing of every frame is kept, besides the ones drawn over the whole output.
    ///
    /// A step is a sequence of frames and a displacement is introduced by one of them, so which one
    /// is the question a whole frame on its own cannot answer. Off by default because it keeps a
    /// transcript per frame.
    pub(crate) transcribe: AtomicBool,
    /// Every frame's drawing since the list was last emptied, with what it was drawn against.
    pub(crate) every_list: Mutex<Vec<(String, String)>>,
}

/// Returns what `held` holds, for one read or write.
pub(crate) fn lock<T>(held: &Mutex<T>) -> MutexGuard<'_, T> {
    held.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Capture {
    /// Keeps the draw damage of `frame` and its drawing, as asked for.
    fn observe(&self, frame: &Frame, drawing: &str) {
        let damage = &frame.damage.draw;
        let rects: Vec<Rect<i32, Device>> = damage
            .rects()
            .iter()
            .map(|rect| {
                Rect::new(
                    Point::new(rect.origin.x, rect.origin.y),
                    Size::new(rect.size.width, rect.size.height),
                )
            })
            .collect();
        if damage.is_full() {
            lock(&self.full_lists).push(drawing.to_owned());
        }
        if self.transcribe.load(Ordering::Relaxed) {
            let against = if damage.is_full() {
                "full".to_owned()
            } else {
                format!("{rects:?}")
            };
            lock(&self.every_list).push((against, drawing.to_owned()));
        }
        lock(&self.damages).push((damage.is_full(), rects));
    }
}

/// This machine's device, with each frame transcribed and a readback taken whenever one is asked
/// for.
pub(crate) struct Recorded {
    inner: WgpuRenderer,
    mirror: CaptureRenderer,
    capture: Arc<Capture>,
}

impl Recorded {
    /// Returns `inner` recording into `capture`.
    pub(crate) fn new(inner: WgpuRenderer, capture: Arc<Capture>) -> Self {
        let mirror = CaptureRenderer::with_capabilities(inner.capabilities());
        Self {
            inner,
            mirror,
            capture,
        }
    }
}

impl Renderer for Recorded {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
        self.mirror.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    fn target_age(&self) -> BufferAge {
        crate::draw::age(self.inner.target_age())
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        self.mirror.draw(frame);
        let outcome = self.inner.draw(frame);
        crate::draw::drew();
        if let Some(captured) = self.mirror.last() {
            self.capture.observe(frame, captured.drawing.as_str());
        }
        if self.capture.want.load(Ordering::Relaxed) {
            *lock(&self.capture.last) = self.inner.read_presented();
            *lock(&self.capture.composed) = Some(self.inner.read_composed());
        }
        outcome
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn vector_status(&self) -> VectorStatus {
        self.inner.vector_status()
    }
}
