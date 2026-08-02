//! A value moved from one number to another over a fixed duration.
//!
//! The predictable half of the non-CSS driver: a tween's end time is known when it starts, which is
//! what a caller needs when something else has to happen exactly when the motion finishes.

use core::time::Duration;

/// How a tween's progress is shaped between its ends.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Easing {
    /// Constant speed throughout.
    Linear,
    /// Slow at the start, fast at the end.
    In,
    /// Fast at the start, slow at the end.
    Out,
    /// Slow at both ends.
    #[default]
    InOut,
}

impl Easing {
    /// The shaped progress for a linear progress in `0..=1`.
    pub fn at(self, progress: f32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => progress,
            Self::In => progress * progress * progress,
            Self::Out => {
                let remaining = 1.0 - progress;
                1.0 - remaining * remaining * remaining
            }
            Self::InOut => {
                if progress < 0.5 {
                    4.0 * progress * progress * progress
                } else {
                    let remaining = -2.0 * progress + 2.0;
                    1.0 - remaining * remaining * remaining / 2.0
                }
            }
        }
    }
}

/// A value crossing from one number to another over a known length of time.
///
/// ```
/// use core::time::Duration;
/// use zgui_anim::motion::{Easing, Tween};
///
/// let mut tween = Tween::new(0.0, 10.0, Duration::from_millis(100)).with_easing(Easing::Linear);
/// assert_eq!(tween.value(), 0.0);
/// tween.advance(Duration::from_millis(50));
/// assert!((tween.value() - 5.0).abs() < 0.01);
/// tween.advance(Duration::from_millis(50));
/// assert_eq!(tween.value(), 10.0);
/// assert!(tween.is_finished());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    /// Where it started.
    from: f32,
    /// Where it is going.
    to: f32,
    /// How long the crossing takes.
    duration: Duration,
    /// How much of that has passed.
    elapsed: Duration,
    /// How the progress is shaped.
    easing: Easing,
}

impl Tween {
    /// A tween from `from` to `to` over `duration`, not yet advanced.
    ///
    /// A zero duration is a jump: the value is `to` from the first frame, which is what a caller
    /// disabling motion wants and is one branch rather than a division by zero.
    pub fn new(from: f32, to: f32, duration: Duration) -> Self {
        Self {
            from,
            to,
            duration,
            elapsed: Duration::ZERO,
            easing: Easing::default(),
        }
    }

    /// The same tween with a different shape.
    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Where the value is now.
    pub fn value(&self) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let progress = self.elapsed.as_secs_f32() / self.duration.as_secs_f32();
        if progress >= 1.0 {
            return self.to;
        }
        self.from + (self.to - self.from) * self.easing.at(progress)
    }

    /// Whether the value has arrived.
    pub fn is_finished(&self) -> bool {
        self.elapsed >= self.duration
    }

    /// Advances by `elapsed` and reports where the value now is.
    pub fn advance(&mut self, elapsed: Duration) -> f32 {
        self.elapsed = (self.elapsed + elapsed).min(self.duration);
        self.value()
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::{Easing, Tween};

    #[test]
    fn every_easing_starts_at_nothing_and_ends_at_everything() {
        for easing in [Easing::Linear, Easing::In, Easing::Out, Easing::InOut] {
            assert_eq!(easing.at(0.0), 0.0, "{easing:?}");
            assert_eq!(easing.at(1.0), 1.0, "{easing:?}");
        }
    }

    #[test]
    fn easing_never_leaves_its_ends_behind() {
        // A caller may hand this a progress past its end after a dropped frame, and a shaped
        // progress outside `0..=1` is a value that overshoots and then snaps back.
        for easing in [Easing::Linear, Easing::In, Easing::Out, Easing::InOut] {
            assert_eq!(easing.at(-1.0), 0.0, "{easing:?}");
            assert_eq!(easing.at(4.0), 1.0, "{easing:?}");
        }
    }

    #[test]
    fn a_zero_length_tween_is_a_jump_and_not_a_division() {
        let tween = Tween::new(0.0, 1.0, Duration::ZERO);
        assert_eq!(tween.value(), 1.0);
        assert!(tween.is_finished());
    }

    #[test]
    fn a_tween_never_passes_its_destination() {
        let mut tween = Tween::new(0.0, 1.0, Duration::from_millis(100));
        assert_eq!(tween.advance(Duration::from_secs(10)), 1.0);
        assert!(tween.is_finished());
    }

    #[test]
    fn ease_out_is_ahead_of_ease_in_at_the_halfway_point() {
        assert!(Easing::Out.at(0.5) > Easing::Linear.at(0.5));
        assert!(Easing::In.at(0.5) < Easing::Linear.at(0.5));
    }
}
