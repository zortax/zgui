//! A mass on a spring, integrated one frame at a time.
//!
//! A spring is the one motion model whose target can change mid-flight without a discontinuity: the
//! value and the velocity are both carried, so retargeting keeps whatever momentum the value had.
//! That is exactly what a dragged sheet needs when the finger lifts, and it is the reason this
//! exists beside a tween rather than instead of one.

use core::time::Duration;

/// How close to its target a spring has to be, and how slow, before it is treated as arrived.
///
/// Both are needed. A spring is arbitrarily close to its target at the top of every overshoot while
/// travelling at full speed, so a distance test on its own stops the motion at the first crossing —
/// a bounce that never bounces.
const AT_REST: f32 = 0.001;

/// The largest step the integrator takes at once, in seconds.
///
/// A frame the compositor dropped can hand this a tenth of a second, and a spring integrated in one
/// step that large is not slow — it is unstable, and it leaves the screen. Splitting it costs a few
/// iterations on the frames that were already late.
const MAX_STEP: f32 = 1.0 / 120.0;

/// A value that follows a target under spring physics.
///
/// ```
/// use core::time::Duration;
/// use zgui_anim::Spring;
///
/// let mut spring = Spring::new(0.0);
/// spring.retarget(1.0);
/// // It sets off towards its target rather than jumping to it.
/// spring.advance(Duration::from_millis(16));
/// assert!(spring.value() > 0.0 && spring.value() < 1.0);
///
/// // And it gets there, and stops.
/// for _ in 0..600 {
///     spring.advance(Duration::from_millis(16));
/// }
/// assert!(spring.is_at_rest());
/// assert!((spring.value() - 1.0).abs() < 0.01);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// Where the value is now.
    value: f32,
    /// How fast it is moving, per second.
    velocity: f32,
    /// Where it is heading.
    target: f32,
    /// How hard the spring pulls.
    stiffness: f32,
    /// How much the motion is resisted.
    damping: f32,
}

impl Spring {
    /// A spring at rest at `value`, with a firm, barely overshooting response.
    pub fn new(value: f32) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            stiffness: 170.0,
            damping: 26.0,
        }
    }

    /// The same spring with a different response.
    ///
    /// `stiffness` is how hard it pulls and `damping` is how much that is resisted. The motion
    /// overshoots when `damping` is below `2 * sqrt(stiffness)` and crawls in when it is above.
    pub fn with_response(mut self, stiffness: f32, damping: f32) -> Self {
        self.stiffness = stiffness.max(0.0);
        self.damping = damping.max(0.0);
        self
    }

    /// Where the value is now.
    pub fn value(&self) -> f32 {
        self.value
    }

    /// How fast it is moving, per second.
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    /// Sends the value somewhere else without interrupting it.
    ///
    /// The velocity is kept, which is the whole point of a spring: a sheet that was being flung
    /// upwards when the finger left carries on upwards and then comes back, rather than stopping
    /// dead and starting again.
    pub fn retarget(&mut self, target: f32) {
        self.target = target;
    }

    /// Puts the value somewhere immediately, with no motion at all.
    pub fn reset(&mut self, value: f32) {
        self.value = value;
        self.target = value;
        self.velocity = 0.0;
    }

    /// Gives the value a push, in units per second.
    pub fn nudge(&mut self, velocity: f32) {
        self.velocity += velocity;
    }

    /// Whether the value has arrived and stopped.
    pub fn is_at_rest(&self) -> bool {
        (self.target - self.value).abs() < AT_REST && self.velocity.abs() < AT_REST
    }

    /// Advances by `elapsed` and reports where the value now is.
    ///
    /// A spring already at rest costs one test and does nothing, which is what makes calling this
    /// unconditionally from a frame loop free.
    pub fn advance(&mut self, elapsed: Duration) -> f32 {
        if self.is_at_rest() {
            self.value = self.target;
            self.velocity = 0.0;
            return self.value;
        }
        let mut remaining = elapsed.as_secs_f32();
        while remaining > 0.0 {
            let step = remaining.min(MAX_STEP);
            let acceleration =
                self.stiffness * (self.target - self.value) - self.damping * self.velocity;
            self.velocity += acceleration * step;
            self.value += self.velocity * step;
            remaining -= step;
        }
        if self.is_at_rest() {
            self.value = self.target;
            self.velocity = 0.0;
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::Spring;

    /// One frame at sixty per second.
    const FRAME: Duration = Duration::from_nanos(16_666_667);

    #[test]
    fn a_spring_at_its_target_never_moves() {
        let mut spring = Spring::new(3.0);
        assert!(spring.is_at_rest());
        assert_eq!(spring.advance(FRAME), 3.0);
    }

    #[test]
    fn a_retargeted_spring_arrives_and_stops() {
        let mut spring = Spring::new(0.0);
        spring.retarget(100.0);
        for _ in 0..600 {
            spring.advance(FRAME);
        }
        assert!(spring.is_at_rest(), "value {}", spring.value());
        assert_eq!(spring.value(), 100.0);
    }

    #[test]
    fn a_dropped_frame_does_not_throw_the_value_off_the_screen() {
        // The failure this closes is not slowness. Integrated in one step, a tenth of a second is
        // past the stability limit of this integrator and the value diverges — the sheet leaves
        // the window and never comes back.
        let mut split = Spring::new(0.0);
        split.retarget(1.0);
        split.advance(Duration::from_millis(100));
        assert!(split.value().abs() < 4.0, "value {}", split.value());
    }

    #[test]
    fn momentum_survives_a_retarget() {
        let mut spring = Spring::new(0.0);
        spring.retarget(1.0);
        for _ in 0..4 {
            spring.advance(FRAME);
        }
        let moving = spring.velocity();
        assert!(moving > 0.0);
        spring.retarget(0.5);
        assert_eq!(
            spring.velocity(),
            moving,
            "a retarget stopped the value dead"
        );
    }

    #[test]
    fn a_softer_spring_gets_there_later() {
        let firm = {
            let mut spring = Spring::new(0.0);
            spring.retarget(1.0);
            spring.advance(FRAME * 4);
            spring.value()
        };
        let soft = {
            let mut spring = Spring::new(0.0).with_response(40.0, 26.0);
            spring.retarget(1.0);
            spring.advance(FRAME * 4);
            spring.value()
        };
        assert!(soft < firm, "soft {soft} firm {firm}");
    }
}
