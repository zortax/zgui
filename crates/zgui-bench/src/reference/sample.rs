//! One point of a slope, taken so that it is a property of the code rather than of the scheduler.

use std::time::Duration;

/// How many times an interaction is driven before anything is recorded.
///
/// The first pass through an interaction faults in pages, fills the branch predictors and warms
/// every cache the pipeline has. Measuring it measures the machine's first impression of the code.
pub const WARMUP: usize = 12;

/// How many times it is driven with the clock running.
///
/// The *median* of these is what a point of a slope is, not the mean: one sample of an interaction
/// is occasionally the scheduler's rather than the framework's, and a mean carries that outlier
/// into the slope for ever.
pub const REPEATS: usize = 48;

/// The median of `samples`, which are sorted in place.
///
/// # Panics
///
/// Panics when `samples` is empty, because the median of nothing is not a measurement that came
/// out small — it is a measurement that did not happen, and a workload that reports it as a number
/// reports a slope through points it never took.
#[must_use]
pub fn median(samples: &mut [f64]) -> f64 {
    assert!(
        !samples.is_empty(),
        "no samples were taken, so there is no median to report"
    );
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

/// Drives `once` [`WARMUP`] times discarding the result, then [`REPEATS`] times, and returns the
/// median of the timed passes in **microseconds**.
///
/// `turn` counts from zero across both, so an interaction that must not repeat itself — a configure
/// restating the size the window already has, a wheel notch delivered where the list is already
/// scrolled to — can vary itself by it and still be warmed by the same code that measures it.
pub fn median_us(mut once: impl FnMut(usize) -> Duration) -> f64 {
    for turn in 0..WARMUP {
        once(turn);
    }
    let mut samples: Vec<f64> = (0..REPEATS)
        .map(|turn| once(turn + WARMUP).as_secs_f64() * 1e6)
        .collect();
    median(&mut samples)
}

/// The same in **nanoseconds**, for the interactions whose whole cost is a few microseconds and
/// whose slope per box is therefore a fraction of one.
pub fn median_ns(mut once: impl FnMut(usize) -> Duration) -> f64 {
    for turn in 0..WARMUP {
        once(turn);
    }
    let mut samples: Vec<f64> = (0..REPEATS)
        .map(|turn| once(turn + WARMUP).as_secs_f64() * 1e9)
        .collect();
    median(&mut samples)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{REPEATS, WARMUP, median, median_ns, median_us};

    #[test]
    fn the_median_is_the_middle_and_not_the_mean() {
        // The distribution a timing actually has: a cluster and one sample the scheduler owns.
        let mut samples = [10.0, 11.0, 10.5, 9.8, 4_000.0];
        assert!((median(&mut samples) - 10.5).abs() < 1e-9);
    }

    #[test]
    #[should_panic(expected = "no samples were taken")]
    fn nothing_measured_is_not_a_measurement_of_zero() {
        let _ = median(&mut []);
    }

    #[test]
    fn the_warm_up_passes_are_not_in_the_answer() {
        // Every warm-up pass is slow and every timed pass is fast, which is the shape a warm-up
        // exists for. A median that included them would land between the two.
        let median = median_us(|turn| {
            if turn < WARMUP {
                Duration::from_micros(1_000)
            } else {
                Duration::from_micros(10)
            }
        });
        assert!((median - 10.0).abs() < 1e-6, "{median}");
    }

    #[test]
    fn the_turn_keeps_counting_across_the_warm_up() {
        let mut seen = Vec::new();
        median_ns(|turn| {
            seen.push(turn);
            Duration::from_nanos(1)
        });
        assert_eq!(seen.len(), WARMUP + REPEATS);
        assert_eq!(seen.first().copied(), Some(0));
        assert_eq!(seen.last().copied(), Some(WARMUP + REPEATS - 1));
    }
}
