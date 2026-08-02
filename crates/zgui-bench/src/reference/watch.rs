//! What each frame damaged, recorded as a fraction of the surface.
//!
//! A damage fraction is dimensionless before anybody divides anything: it is the share of the
//! window a frame asked the renderer to redraw, and it is the same number on any machine. That
//! makes it one of the few things a reference workload can gate directly. It is also the only
//! evidence that separates a scroll which moved the content from a scroll which redrew the page —
//! two frames that take the same time on a machine fast enough and diverge on every other.
//!
//! The recorder wraps a real renderer rather than replacing it, so the damage it sees is the damage
//! the pipeline actually produced, and the frames it counts are the ones the renderer accepted: a
//! frame identical to the last damages nothing and is refused, and counting a refused frame would
//! report a fraction the surface never showed.

use std::cell::RefCell;
use std::rc::Rc;

use zgui::bits::DamageSet;
use zgui::render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui::scene::Scene;

/// What one drawn frame damaged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Damaged {
    /// The share of the surface inside the damage rectangles, between zero and one.
    ///
    /// A full-damage frame is one. A frame whose rectangles overlap can in principle sum past the
    /// surface, so the share is clamped: the question is "how much of the window did this cost",
    /// and no frame costs more than the window.
    pub fraction: f64,
    /// Whether the frame declared the whole surface damaged.
    pub full: bool,
    /// How many rectangles the damage was made of.
    ///
    /// Read beside the fraction. `MAX_DAMAGE` rectangles is the point at which the set collapses
    /// into their union, so a workload whose fraction climbed *and* whose rectangle count sits at
    /// the cap has run out of room to describe what it changed rather than changed more.
    pub rects: usize,
}

/// The frames a run drew, in order.
pub type Log = Rc<RefCell<Vec<Damaged>>>;

/// The mean damage fraction over `log`, or `None` when nothing was drawn.
///
/// `None` rather than zero, because a run that drew nothing damaged nothing, and "this workload
/// damages almost none of the surface" is exactly what a broken workload would also report.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "the divisor is a frame count in the hundreds"
)]
pub fn mean_fraction(log: &Log) -> Option<f64> {
    let frames = log.borrow();
    (!frames.is_empty())
        .then(|| frames.iter().map(|frame| frame.fraction).sum::<f64>() / frames.len() as f64)
}

/// How many of the frames in `log` declared the whole surface damaged.
#[must_use]
pub fn full_frames(log: &Log) -> usize {
    log.borrow().iter().filter(|frame| frame.full).count()
}

/// A renderer that records what every accepted frame damaged and then draws it.
pub struct Watching {
    /// What actually draws.
    inner: Box<dyn Renderer>,
    /// Where the record goes.
    log: Log,
    /// The surface's area in device pixels, from the last [`Renderer::configure`].
    surface: f64,
}

impl Watching {
    /// Wraps `inner`, recording into `log`.
    #[must_use]
    pub fn new(inner: Box<dyn Renderer>, log: Log) -> Self {
        Self {
            inner,
            log,
            surface: 0.0,
        }
    }
}

impl Renderer for Watching {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.surface = f64::from(target.size.width) * f64::from(target.size.height);
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "a damaged area in device pixels is bounded by the surface, which is millions"
    )]
    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let outcome = self.inner.draw(scene, damage);
        if matches!(outcome, FrameOutcome::Presented(_)) && self.surface > 0.0 {
            let fraction = if damage.is_full() {
                1.0
            } else {
                (damage.area().unwrap_or(0) as f64 / self.surface).clamp(0.0, 1.0)
            };
            self.log.borrow_mut().push(Damaged {
                fraction,
                full: damage.is_full(),
                rects: damage.rects().len(),
            });
        }
        outcome
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        self.inner.texture_sink()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::{Damaged, Log, full_frames, mean_fraction};

    fn log_of(fractions: &[(f64, bool)]) -> Log {
        Rc::new(RefCell::new(
            fractions
                .iter()
                .map(|&(fraction, full)| Damaged {
                    fraction,
                    full,
                    rects: 1,
                })
                .collect(),
        ))
    }

    #[test]
    fn the_mean_is_over_the_frames_that_were_drawn() {
        let log = log_of(&[(0.1, false), (0.3, false)]);
        assert!((mean_fraction(&log).unwrap() - 0.2).abs() < 1e-12);
    }

    #[test]
    fn a_run_that_drew_nothing_reports_nothing_rather_than_no_damage() {
        // Read as zero, a run that never drew is indistinguishable from the best possible run.
        assert_eq!(mean_fraction(&log_of(&[])), None);
    }

    #[test]
    fn full_frames_are_counted_apart_from_the_mean() {
        let log = log_of(&[(1.0, true), (0.02, false), (1.0, true)]);
        assert_eq!(full_frames(&log), 2);
    }
}
