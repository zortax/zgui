//! Turning numbers into pixels, and choosing the numbers an axis is labelled with.

/// A linear map from a range of values onto a range of pixels.
///
/// One direction only, and deliberately: a chart maps data to the plot area, and the inverse — which
/// value is under the pointer — is [`Scale::invert`] rather than a second scale that can drift out
/// of step with the first.
///
/// ```
/// use zgui_ui::chart::Scale;
///
/// // Zero to a hundred, over two hundred pixels.
/// let scale = Scale::new(0.0, 100.0, 0.0, 200.0);
/// assert_eq!(scale.at(50.0), 100.0);
/// assert_eq!(scale.invert(100.0), 50.0);
///
/// // Downwards, which is what a value axis is: bigger numbers are nearer the top of the screen.
/// let value = Scale::new(0.0, 100.0, 200.0, 0.0);
/// assert_eq!(value.at(100.0), 0.0);
/// ```
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Scale {
    /// The smallest value.
    min: f64,
    /// The largest.
    max: f64,
    /// Where the smallest value sits, in pixels.
    start: f64,
    /// Where the largest value sits, in pixels.
    end: f64,
}

impl Scale {
    /// A scale mapping `min`..`max` onto `start`..`end`.
    ///
    /// An empty domain is widened by one unit rather than dividing by zero: a chart of a single
    /// value is a real chart, and it draws that value in the middle.
    #[must_use]
    pub fn new(min: f64, max: f64, start: f64, end: f64) -> Self {
        let (min, max) = if (max - min).abs() < f64::EPSILON {
            (min - 0.5, max + 0.5)
        } else if max < min {
            (max, min)
        } else {
            (min, max)
        };
        Self {
            min,
            max,
            start,
            end,
        }
    }

    /// A scale over everything in `values`, extended to include zero.
    ///
    /// Including zero is what makes a bar chart honest: bars measured from the smallest value
    /// rather than from zero exaggerate every difference between them.
    #[must_use]
    pub fn over(values: impl IntoIterator<Item = f64>, start: f64, end: f64) -> Self {
        let mut min = 0.0_f64;
        let mut max = 0.0_f64;
        let mut seen = false;
        for value in values {
            if !value.is_finite() {
                continue;
            }
            if seen {
                min = min.min(value);
                max = max.max(value);
            } else {
                min = value.min(0.0);
                max = value.max(0.0);
                seen = true;
            }
        }
        Self::new(min, max, start, end)
    }

    /// The smallest value on the scale.
    #[must_use]
    pub const fn min(&self) -> f64 {
        self.min
    }

    /// The largest.
    #[must_use]
    pub const fn max(&self) -> f64 {
        self.max
    }

    /// Where `value` sits, in pixels, clamped to the ends of the scale.
    #[must_use]
    pub fn at(&self, value: f64) -> f64 {
        let fraction = ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0);
        self.start + fraction * (self.end - self.start)
    }

    /// Which value sits at `pixel`.
    #[must_use]
    pub fn invert(&self, pixel: f64) -> f64 {
        if (self.end - self.start).abs() < f64::EPSILON {
            return self.min;
        }
        let fraction = ((pixel - self.start) / (self.end - self.start)).clamp(0.0, 1.0);
        self.min + fraction * (self.max - self.min)
    }

    /// Roughly `count` values to label the axis with, at round numbers.
    ///
    /// Round in the sense a reader means: the step is one, two or five times a power of ten, so an
    /// axis is labelled 0, 25, 50, 75, 100 rather than 0, 23.4, 46.8 — and the labels stay the same
    /// as the data grows, which is what stops an axis flickering as a live chart updates.
    ///
    /// ```
    /// use zgui_ui::chart::Scale;
    ///
    /// let scale = Scale::new(0.0, 97.0, 0.0, 100.0);
    /// assert_eq!(scale.ticks(5), vec![0.0, 20.0, 40.0, 60.0, 80.0]);
    /// ```
    #[must_use]
    pub fn ticks(&self, count: usize) -> Vec<f64> {
        let count = count.max(1);
        let step = nice_step((self.max - self.min) / count as f64);
        if step <= 0.0 {
            return vec![self.min];
        }
        let first = (self.min / step).ceil() * step;
        let mut ticks = Vec::new();
        let mut value = first;
        // Bounded independently of the arithmetic, so a pathological domain cannot spin here.
        while value <= self.max + step * 1e-9 && ticks.len() <= count * 4 {
            // Snapped, because repeated addition of 0.1 does not stay on 0.1 boundaries.
            ticks.push(((value / step).round() * step).clamp(self.min, self.max));
            value += step;
        }
        ticks
    }
}

/// The nearest round number at or above `rough`: one, two or five times a power of ten.
#[must_use]
pub fn nice_step(rough: f64) -> f64 {
    if !rough.is_finite() || rough <= 0.0 {
        return 0.0;
    }
    let magnitude = 10.0_f64.powf(rough.log10().floor());
    let normalised = rough / magnitude;
    let step = if normalised <= 1.0 {
        1.0
    } else if normalised <= 2.0 {
        2.0
    } else if normalised <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * magnitude
}

/// A number as an axis label: as few digits as say what it is.
///
/// Whole numbers lose their point, and everything else keeps enough decimals to distinguish one
/// tick from the next — an axis labelled `0.0000001` at every tick tells a reader nothing.
#[must_use]
pub fn tick_label(value: f64) -> String {
    if !value.is_finite() {
        return String::from("—");
    }
    if (value - value.round()).abs() < 1e-9 {
        return format!("{}", value.round() as i64);
    }
    let magnitude = value.abs();
    let decimals = if magnitude >= 100.0 {
        0
    } else if magnitude >= 1.0 {
        1
    } else {
        3
    };
    format!("{value:.decimals$}")
}

#[cfg(test)]
mod tests {
    use super::{Scale, nice_step, tick_label};

    #[test]
    fn a_value_and_the_pixel_it_lands_on_are_inverses() {
        let scale = Scale::new(-20.0, 80.0, 0.0, 400.0);
        for value in [-20.0, -3.0, 0.0, 17.5, 80.0] {
            let round_trip = scale.invert(scale.at(value));
            assert!(
                (round_trip - value).abs() < 1e-9,
                "{value} came back as {round_trip}",
            );
        }
    }

    #[test]
    fn a_domain_of_one_value_draws_it_rather_than_dividing_by_zero() {
        let flat = Scale::new(7.0, 7.0, 0.0, 100.0);
        assert!(flat.at(7.0).is_finite());
        assert_eq!(flat.at(7.0), 50.0, "the one value is in the middle");
    }

    #[test]
    fn a_bar_charts_scale_always_includes_zero() {
        let scale = Scale::over([80.0, 90.0, 100.0], 0.0, 200.0);
        assert_eq!(scale.min(), 0.0, "bars measured from 80 would exaggerate");
        assert_eq!(scale.max(), 100.0);

        let negatives = Scale::over([-40.0, -10.0], 0.0, 200.0);
        assert_eq!(negatives.min(), -40.0);
        assert_eq!(negatives.max(), 0.0);
    }

    #[test]
    fn a_scale_over_nothing_is_still_a_scale() {
        let empty = Scale::over(Vec::<f64>::new(), 0.0, 100.0);
        assert!(empty.at(0.0).is_finite());
        assert!(!empty.ticks(5).is_empty());
    }

    #[test]
    fn the_ticks_are_numbers_a_reader_would_choose() {
        assert_eq!(nice_step(0.03), 0.05);
        assert_eq!(nice_step(23.0), 50.0);
        assert_eq!(nice_step(1.0), 1.0);
        assert_eq!(nice_step(0.0), 0.0);
        assert_eq!(nice_step(f64::NAN), 0.0);

        let scale = Scale::new(0.0, 1000.0, 0.0, 100.0);
        assert!(scale.ticks(4).contains(&500.0));
    }

    #[test]
    fn every_tick_is_inside_the_scale_it_labels() {
        for (min, max) in [(0.0, 1.0), (-13.0, 91.0), (1e6, 1.000_01e6)] {
            let scale = Scale::new(min, max, 0.0, 100.0);
            for tick in scale.ticks(6) {
                assert!(
                    tick >= scale.min() && tick <= scale.max(),
                    "{tick} is outside {min}..{max}",
                );
            }
        }
    }

    #[test]
    fn a_label_keeps_the_digits_that_matter_and_drops_the_ones_that_do_not() {
        assert_eq!(tick_label(40.0), "40");
        assert_eq!(tick_label(-7.0), "-7");
        assert_eq!(tick_label(0.125), "0.125");
        assert_eq!(tick_label(12.5), "12.5");
        assert_eq!(tick_label(f64::INFINITY), "—");
    }

    #[test]
    fn a_value_outside_the_scale_is_drawn_at_the_edge_rather_than_off_the_chart() {
        let scale = Scale::new(0.0, 10.0, 0.0, 100.0);
        assert_eq!(scale.at(-5.0), 0.0);
        assert_eq!(scale.at(50.0), 100.0);
    }
}
