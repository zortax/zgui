//! The speed a gesture left behind, spending itself over the frames that follow.

use core::time::Duration;

use zgui_geom::{Device, DevicePx, Point, Size};

use crate::motion::{Step, stopped};

/// How much of a fling's speed survives one second.
///
/// Exponential decay rather than a constant deceleration, because a constant one makes a gentle
/// flick stop almost at once while a hard one runs for seconds; the exponential gives both the same
/// *feel* and different distances, which is what a flick means.
const REMAINING_PER_SECOND: f32 = 0.002;

/// A container carrying the speed of the gesture that let go of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Momentum {
    /// Device pixels per second, in the direction the content is moving.
    velocity: Size<DevicePx, Device>,
}

impl Momentum {
    /// A fling at `velocity`, in device pixels per second.
    pub fn new(velocity: Size<DevicePx, Device>) -> Self {
        Self { velocity }
    }

    /// Spends `elapsed` of it, from wherever the container is now.
    pub fn advance(&mut self, at: Point<DevicePx, Device>, elapsed: Duration) -> Step {
        let seconds = elapsed.as_secs_f32();
        let factor = REMAINING_PER_SECOND.powf(seconds);
        // The integral of the decay over the step, which is the distance travelled — using the
        // start speed instead would overshoot by a whole frame's worth on every tick, and a flick
        // would travel measurably further at 30 Hz than at 120 Hz.
        let travelled = if seconds > 0.0 {
            (1.0 - factor) / -REMAINING_PER_SECOND.ln()
        } else {
            0.0
        };
        let to = Point::new(
            DevicePx(at.x.0 + self.velocity.width.0 * travelled),
            DevicePx(at.y.0 + self.velocity.height.0 * travelled),
        );
        self.velocity = Size::new(
            DevicePx(self.velocity.width.0 * factor),
            DevicePx(self.velocity.height.0 * factor),
        );
        Step {
            to,
            done: stopped(self.velocity),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{Device, DevicePx, Point, Size};

    use super::Momentum;

    fn speed(y: f32) -> Size<DevicePx, Device> {
        Size::new(DevicePx(0.0), DevicePx(y))
    }

    fn origin() -> Point<DevicePx, Device> {
        Point::new(DevicePx(0.0), DevicePx(0.0))
    }

    /// How far a fling travels in total, and how many frames it takes.
    fn run(initial: f32) -> (f32, u32) {
        let mut momentum = Momentum::new(speed(initial));
        let mut at = origin();
        let mut frames = 0;
        loop {
            let step = momentum.advance(at, Duration::from_millis(16));
            at = step.to;
            frames += 1;
            if step.done || frames > 600 {
                return (at.y.0, frames);
            }
        }
    }

    #[test]
    fn a_harder_flick_travels_further() {
        let (gentle, _) = run(400.0);
        let (hard, _) = run(2_000.0);
        assert!(hard > gentle * 4.0, "{hard} against {gentle}");
    }

    #[test]
    fn a_fling_stops_rather_than_decaying_for_ever() {
        let (_, frames) = run(2_000.0);
        assert!(frames < 120, "it was still running after {frames} frames");
    }

    #[test]
    fn the_distance_does_not_depend_on_the_frame_rate() {
        let mut fast = Momentum::new(speed(1_000.0));
        let mut slow = Momentum::new(speed(1_000.0));
        let mut fast_at = origin();
        for _ in 0..4 {
            fast_at = fast.advance(fast_at, Duration::from_millis(8)).to;
        }
        let slow_at = slow.advance(origin(), Duration::from_millis(32)).to;
        assert!(
            (fast_at.y.0 - slow_at.y.0).abs() < 1.0,
            "{} against {}: a flick that travels further on a slower display is a flick whose \
             physics is a per-frame constant",
            fast_at.y.0,
            slow_at.y.0
        );
    }

    #[test]
    fn a_fling_that_was_never_thrown_is_already_finished() {
        let mut momentum = Momentum::new(speed(0.0));
        assert!(momentum.advance(origin(), Duration::from_millis(16)).done);
    }
}
