//! Offsets that move over time.
//!
//! Two kinds, and they are not the same mechanism wearing different constants. A **smooth scroll**
//! knows where it is going — it was asked to reach a particular offset — so it interpolates towards
//! a destination over a fixed duration. A **fling** does not: a finger left the surface at a speed,
//! and where the content ends up is whatever that speed carries it to. One is a tween and the other
//! is a decay, and writing the fling as a tween means guessing the destination, which is how a flick
//! comes to overshoot on a short list and undershoot on a long one.

pub mod momentum;
pub mod tween;

use core::time::Duration;

use zgui_geom::{Device, DevicePx, Point, Size};

pub use crate::motion::momentum::Momentum;
pub use crate::motion::tween::Tween;

/// Whether a scroll jumps or travels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Behavior {
    /// Whatever the document asks for, which is to arrive at once until it asks for anything else.
    ///
    /// This is what a caller with no opinion says — an accessibility action bringing a control into
    /// view, a focus move scrolling its target into the scrollport — and it is a third value rather
    /// than a synonym for [`Behavior::Instant`] because the two say different things: one is "no
    /// preference", which the document may yet answer differently, and the other is "do not
    /// animate this", which is an instruction.
    #[default]
    Auto,
    /// Arrive in this frame.
    Instant,
    /// Travel there over the next few.
    Smooth,
}

impl Behavior {
    /// Whether a scroll under this behaviour travels over several frames.
    pub const fn animates(self) -> bool {
        matches!(self, Self::Smooth)
    }
}

/// An offset on its way somewhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    /// Travelling to a known destination.
    Smooth(Tween),
    /// Carrying the speed a gesture left behind.
    Fling(Momentum),
}

/// Where a motion has reached, and whether it has finished.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    /// The offset to write, which is not yet clamped to what the content allows.
    pub to: Point<DevicePx, Device>,
    /// Whether this was the last step.
    pub done: bool,
}

impl Motion {
    /// Advances by `elapsed`, from the offset the container is at now.
    ///
    /// The current offset is an argument rather than remembered state because a motion does not own
    /// the offset: a wheel event, a `scroll_to` or a clamp against content that changed size can all
    /// move it underneath a running fling, and a fling that carried its own copy would jump the
    /// content back to where it thought it was.
    pub fn advance(&mut self, at: Point<DevicePx, Device>, elapsed: Duration) -> Step {
        match self {
            Self::Smooth(tween) => tween.advance(elapsed),
            Self::Fling(momentum) => momentum.advance(at, elapsed),
        }
    }
}

/// A fling with this much speed left, in device pixels per second, has stopped.
///
/// One pixel a second is under a pixel a frame at any refresh rate anyone ships, so continuing costs
/// a frame and moves nothing.
pub(crate) const STOPPED: f32 = 1.0;

/// Whether a velocity is too small to keep asking for frames over.
pub(crate) fn stopped(velocity: Size<DevicePx, Device>) -> bool {
    velocity.width.0.abs() < STOPPED && velocity.height.0.abs() < STOPPED
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{DevicePx, Point, Size};

    use super::{Behavior, Momentum, Motion, Tween};

    #[test]
    fn the_default_behaviour_is_to_have_no_preference_and_it_does_not_animate() {
        assert_eq!(Behavior::default(), Behavior::Auto);
        assert!(!Behavior::Auto.animates());
        assert!(!Behavior::Instant.animates());
        assert!(Behavior::Smooth.animates());
    }

    #[test]
    fn a_fling_reads_the_offset_it_is_given_rather_than_one_it_remembered() {
        let mut motion = Motion::Fling(Momentum::new(Size::new(DevicePx(0.0), DevicePx(1_000.0))));
        // Something else moved the container between ticks — a wheel event, or a clamp.
        let moved = Point::new(DevicePx(0.0), DevicePx(400.0));
        let step = motion.advance(moved, Duration::from_millis(16));
        assert!(
            step.to.y.0 > 400.0,
            "the fling continued from where the container actually is, not from {}",
            step.to.y.0
        );
    }

    #[test]
    fn a_smooth_scroll_ignores_the_offset_it_is_given_because_it_knows_its_destination() {
        let from = Point::new(DevicePx(0.0), DevicePx(0.0));
        let to = Point::new(DevicePx(0.0), DevicePx(100.0));
        let mut motion = Motion::Smooth(Tween::new(from, to));
        let step = motion.advance(
            Point::new(DevicePx(0.0), DevicePx(9_999.0)),
            Tween::DURATION,
        );
        assert_eq!(step.to, to);
        assert!(step.done);
    }
}
