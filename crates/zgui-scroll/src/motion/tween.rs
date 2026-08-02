//! A scroll travelling to a destination it already knows.

use core::time::Duration;

use zgui_geom::{Device, DevicePx, Point};

use crate::motion::Step;

/// A scroll interpolating from where it started to where it was asked to go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    /// Where it started.
    from: Point<DevicePx, Device>,
    /// Where it is going.
    to: Point<DevicePx, Device>,
    /// How much of the duration has passed.
    elapsed: Duration,
}

impl Tween {
    /// How long a smooth scroll takes.
    ///
    /// Long enough to be followed by the eye and short enough that a keyboard user holding the page
    /// key is not fighting the animation. It is a constant rather than a function of distance
    /// because a distance-proportional duration makes a long jump take a visibly silly amount of
    /// time, and the eye is tracking the arrival rather than the speed.
    pub const DURATION: Duration = Duration::from_millis(240);

    /// A scroll from `from` to `to`.
    pub fn new(from: Point<DevicePx, Device>, to: Point<DevicePx, Device>) -> Self {
        Self {
            from,
            to,
            elapsed: Duration::ZERO,
        }
    }

    /// Where it is heading.
    ///
    /// What a second detent adds to, which is the difference between two detents composing into one
    /// motion and the second one cancelling the first: a wheel turned twice in a third of a second
    /// asks to go twice as far, not to go half as far and start again.
    pub fn destination(&self) -> Point<DevicePx, Device> {
        self.to
    }

    /// Sends it somewhere else, continuing from where it has actually reached.
    ///
    /// The ease starts again from `at`, and that is what makes a run of detents feel like one
    /// motion rather than a series of them. The alternative — keeping the elapsed time and moving
    /// only the destination — makes the second detent arrive almost instantly, because the ease is
    /// already most of the way through its curve and the whole of the new distance is covered in
    /// what remains of the old duration.
    ///
    /// ```
    /// use core::time::Duration;
    /// use zgui_geom::{Device, DevicePx, Point};
    /// use zgui_scroll::motion::Tween;
    ///
    /// let at = |y: f32| Point::<DevicePx, Device>::new(DevicePx(0.0), DevicePx(y));
    /// let mut tween = Tween::new(at(0.0), at(100.0));
    /// let reached = tween.advance(Duration::from_millis(60)).to;
    /// tween.retarget(reached, at(200.0));
    /// assert_eq!(tween.destination(), at(200.0));
    /// assert!(tween.advance(Duration::from_millis(16)).to.y.0 > reached.y.0);
    /// ```
    pub fn retarget(&mut self, at: Point<DevicePx, Device>, to: Point<DevicePx, Device>) {
        self.from = at;
        self.to = to;
        self.elapsed = Duration::ZERO;
    }

    /// Advances it by `elapsed`.
    pub fn advance(&mut self, elapsed: Duration) -> Step {
        self.elapsed = (self.elapsed + elapsed).min(Self::DURATION);
        let progress = self.elapsed.as_secs_f32() / Self::DURATION.as_secs_f32();
        let done = self.elapsed >= Self::DURATION;
        if done {
            return Step {
                to: self.to,
                done: true,
            };
        }
        let eased = ease(progress);
        Step {
            to: Point::new(
                DevicePx(lerp(self.from.x.0, self.to.x.0, eased)),
                DevicePx(lerp(self.from.y.0, self.to.y.0, eased)),
            ),
            done: false,
        }
    }
}

/// Fast at first and slow at the end, which is what makes an arrival read as an arrival.
fn ease(progress: f32) -> f32 {
    let remaining = 1.0 - progress;
    1.0 - remaining * remaining * remaining
}

/// One axis of the interpolation.
fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{Device, DevicePx, Point};

    use super::Tween;

    fn at(y: f32) -> Point<DevicePx, Device> {
        Point::new(DevicePx(0.0), DevicePx(y))
    }

    #[test]
    fn it_arrives_exactly_and_reports_that_it_has() {
        let mut tween = Tween::new(at(0.0), at(400.0));
        let step = tween.advance(Tween::DURATION);
        assert_eq!(step.to, at(400.0));
        assert!(step.done);
    }

    #[test]
    fn an_overlong_step_does_not_overshoot() {
        let mut tween = Tween::new(at(0.0), at(400.0));
        let step = tween.advance(Duration::from_secs(10));
        assert_eq!(step.to, at(400.0));
        assert!(step.done);
    }

    #[test]
    fn it_covers_more_ground_early_than_late() {
        let mut early = Tween::new(at(0.0), at(400.0));
        let first = early.advance(Duration::from_millis(60)).to.y.0;
        let second = early.advance(Duration::from_millis(60)).to.y.0 - first;
        assert!(
            first > second,
            "the first quarter covered {first} and the second {second}, which is not a deceleration"
        );
    }

    #[test]
    fn it_moves_on_the_very_first_frame() {
        // A tween that eased in as well as out would move a sub-pixel distance in the first frame,
        // and a scroll that visibly does nothing for two frames after a key press reads as a
        // dropped key press rather than as a smooth scroll.
        let mut tween = Tween::new(at(0.0), at(400.0));
        assert!(tween.advance(Duration::from_millis(16)).to.y.0 > 4.0);
    }
}
