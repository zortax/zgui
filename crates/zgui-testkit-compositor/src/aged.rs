//! A recording renderer whose presenter composes into a rotation of images.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use compositor_api::{Presenter, Renderer};
use compositor_capture::CaptureRenderer;
use compositor_types::compositor_damage::DamageSet;
use compositor_types::{
    Acquired, BufferAge, Frame, FrameOutcome, MemoryReport, PresentCapabilities, PresentFeedback,
    PresentMode, PresentResult, RenderCapabilities, RenderTarget, STALE_DAMAGE, SkipReason,
};

use crate::drawing::Drawing;
use crate::kept::{Drawn, Frames};

/// A renderer in buffer-age mode: each frame is composed into the image its presenter hands out,
/// and the image's age says which earlier frames it already holds.
///
/// The presenter rotates through a fixed number of images in order. A new or reconfigured image
/// has an unknown age, and an image that holds the frame presented `n` frames ago has the age `n`. Every frame drawn is
/// recorded with its damage, as [`Kept`](crate::Kept) records them.
#[derive(Debug)]
pub struct Aged {
    capture: CaptureRenderer,
    presenter: Rotation,
    frames: Frames,
}

/// How many acquisitions of an [`Aged`] renderer fail before the next one succeeds, readable and
/// settable after the renderer has been handed away.
#[derive(Clone, Debug, Default)]
pub struct Refusals(Arc<AtomicU32>);

impl Refusals {
    /// Makes the next `count` acquisitions fail as `Unacquired`.
    pub fn set(&self, count: u32) {
        self.0.store(count, Ordering::Relaxed);
    }

    /// Returns how many acquisitions are still to fail.
    pub fn left(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }

    /// Takes one refusal, and reports whether there was one to take.
    fn take(&self) -> bool {
        self.0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                left.checked_sub(1)
            })
            .is_ok()
    }
}

/// The images an [`Aged`] renderer composes into, in the order they are handed out.
#[derive(Debug)]
struct Rotation {
    /// When each image was last presented, as a count of presents, or `None` before it was.
    presented: Vec<Option<u64>>,
    /// The image handed out next.
    next: usize,
    /// The image acquired and neither presented nor released.
    acquired: Option<u32>,
    /// How many images were presented.
    presents: u64,
    /// How many acquisitions are refused before the next one succeeds.
    refusals: Refusals,
    /// The empty debt every image owes in buffer-age mode.
    debt: DamageSet<STALE_DAMAGE>,
}

impl Aged {
    /// Returns a renderer with the capabilities of `RenderCapabilities::MINIMAL` that composes
    /// into `images` images, records its frames into `frames` and refuses acquisitions as
    /// `refusals` says.
    pub fn new(images: usize, frames: Frames, refusals: Refusals) -> Self {
        Self {
            capture: CaptureRenderer::new(),
            presenter: Rotation {
                presented: vec![None; images.max(1)],
                next: 0,
                acquired: None,
                presents: 0,
                refusals,
                debt: DamageSet::new(),
            },
            frames,
        }
    }
}

impl Renderer for Aged {
    fn capabilities(&self) -> RenderCapabilities {
        self.capture.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.capture.configure(target);
        let _ = self.presenter.configure(target.size, target.opaque);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.capture.target()
    }

    fn target_age(&self) -> BufferAge {
        BufferAge::Unknown
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        let Some(image) = self.presenter.acquired else {
            return FrameOutcome::Skipped(SkipReason::Unacquired);
        };
        let outcome = self.capture.draw(frame);
        if let Some(captured) = self.capture.last() {
            self.frames.push(Drawn {
                damage: frame.damage,
                shifts: frame.shifts.to_vec(),
                drawing: Drawing::of(captured.drawing.clone()),
            });
        }
        match self.presenter.present(image, &frame.damage.present) {
            PresentResult::Presented => outcome,
            PresentResult::Held => FrameOutcome::Skipped(SkipReason::Held),
            PresentResult::Skipped(reason) => FrameOutcome::Skipped(reason),
        }
    }

    fn presenter(&mut self) -> Option<&mut dyn Presenter> {
        Some(&mut self.presenter)
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

impl Presenter for Rotation {
    fn capabilities(&self) -> PresentCapabilities {
        PresentCapabilities {
            mode: PresentMode::BufferAge,
            partial_present: true,
            feedback: false,
        }
    }

    fn configure(&mut self, _size: [u32; 2], _opaque: bool) -> Result<(), SkipReason> {
        self.presented.fill(None);
        self.acquired = None;
        Ok(())
    }

    fn acquire(&mut self) -> Acquired {
        if self.refusals.take() {
            return Acquired::Skipped(SkipReason::Unacquired);
        }
        let image = self.next;
        self.next = (self.next + 1) % self.presented.len();
        self.acquired = Some(image as u32);
        let age = self.presented[image].map_or(BufferAge::Unknown, |at| {
            BufferAge::Known(u32::try_from(self.presents - at + 1).unwrap_or(u32::MAX))
        });
        Acquired::Image {
            image: image as u32,
            age,
        }
    }

    fn acquired(&self) -> Option<u32> {
        self.acquired
    }

    fn release(&mut self, image: u32) {
        if self.acquired == Some(image) {
            self.acquired = None;
            // The image goes back to the front of the rotation, so it is handed out next.
            self.next = image as usize;
        }
    }

    fn composed(&mut self, _damage: &DamageSet) {}

    fn debt(&self, _image: u32) -> &DamageSet<STALE_DAMAGE> {
        &self.debt
    }

    fn present(&mut self, image: u32, _damage: &DamageSet) -> PresentResult {
        self.acquired = None;
        self.presents += 1;
        if let Some(at) = self.presented.get_mut(image as usize) {
            *at = Some(self.presents);
        }
        PresentResult::Presented
    }

    fn next_deadline(&self) -> Option<u64> {
        None
    }

    fn feedback(&mut self, _out: &mut Vec<PresentFeedback>) {}
}
