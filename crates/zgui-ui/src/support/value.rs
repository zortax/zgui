//! Numbers a control has to keep inside a range.

/// The range a numeric control's value lives in.
///
/// One type rather than three props passed around in a tuple, because the three are only ever
/// meaningful together: a step with no range to walk is a number, and a range with no step is a
/// slider nobody can operate from the keyboard.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Bound {
    /// The smallest value.
    pub min: f64,
    /// The largest.
    pub max: f64,
    /// How far one keystroke moves it.
    pub step: f64,
}

impl Bound {
    /// A range, with the step it moves in.
    #[must_use]
    pub const fn new(min: f64, max: f64, step: f64) -> Self {
        Self { min, max, step }
    }

    /// Where `value` sits in the range, as a fraction from zero to one.
    ///
    /// An empty range answers zero rather than dividing by it, which is what a slider whose bounds
    /// have not been decided yet has.
    #[must_use]
    pub fn fraction(&self, value: f64) -> f64 {
        let span = self.max - self.min;
        if span <= 0.0 {
            return 0.0;
        }
        ((value - self.min) / span).clamp(0.0, 1.0)
    }

    /// The value at `fraction` of the way along the range, snapped to the step.
    #[must_use]
    pub fn at(&self, fraction: f64) -> f64 {
        clamp_to_step(
            self.min + fraction.clamp(0.0, 1.0) * (self.max - self.min),
            *self,
        )
    }
}

impl Default for Bound {
    fn default() -> Self {
        Self::new(0.0, 100.0, 1.0)
    }
}

/// Puts `value` inside `bound` and on one of its steps.
///
/// Snapping is measured from the minimum rather than from zero, because a range starting at 3 with
/// a step of 5 has values 3, 8, 13 — snapping to multiples of five would put every one of them
/// somewhere the range does not go.
///
/// ```
/// use zgui_ui::support::{Bound, clamp_to_step};
///
/// let bound = Bound::new(3.0, 23.0, 5.0);
/// assert_eq!(clamp_to_step(9.0, bound), 8.0);
/// assert_eq!(clamp_to_step(-4.0, bound), 3.0);
/// assert_eq!(clamp_to_step(99.0, bound), 23.0);
/// ```
#[must_use]
pub fn clamp_to_step(value: f64, bound: Bound) -> f64 {
    let clamped = value.clamp(bound.min, bound.max);
    if bound.step <= 0.0 {
        return clamped;
    }
    let steps = ((clamped - bound.min) / bound.step).round();
    (bound.min + steps * bound.step).clamp(bound.min, bound.max)
}

#[cfg(test)]
mod tests {
    use super::{Bound, clamp_to_step};

    #[test]
    fn a_fraction_and_the_value_at_it_are_inverses_on_a_step_boundary() {
        let bound = Bound::new(0.0, 50.0, 5.0);
        for value in [0.0, 5.0, 25.0, 50.0] {
            assert_eq!(bound.at(bound.fraction(value)), value);
        }
    }

    #[test]
    fn a_range_with_no_span_answers_rather_than_dividing_by_zero() {
        let bound = Bound::new(7.0, 7.0, 1.0);
        assert_eq!(bound.fraction(7.0), 0.0);
        assert_eq!(bound.at(0.5), 7.0);
    }

    #[test]
    fn a_step_of_zero_clamps_and_snaps_to_nothing() {
        let bound = Bound::new(0.0, 1.0, 0.0);
        assert_eq!(clamp_to_step(0.371, bound), 0.371);
        assert_eq!(clamp_to_step(2.0, bound), 1.0);
    }
}
