//! Driving the two interactions and timing them.

use std::time::{Duration, Instant};

use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::{Get, Set};
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::runtime::Runtime;
use zgui_platform_headless::Harness;

use super::{HEIGHT, WIDTH};

/// How many times each interaction is driven.
///
/// The median of these is what is reported, not the mean: one sample of an interaction is
/// occasionally the scheduler's rather than the framework's, and a mean carries that outlier into
/// the slope for ever.
const REPEATS: usize = 48;

/// How many of the repeats are thrown away before anything is recorded.
///
/// The first pass through an interaction faults in pages, fills the branch predictors and warms
/// every cache the pipeline has. Measuring it measures the machine's first impression of the code.
const WARMUP: usize = 12;

/// The median of `samples`, which are sorted in place.
fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

/// Delivers one configure and drives the loop until it goes quiet, returning how long that took.
fn configure(harness: &mut Harness<Runtime>, width: f32) -> Duration {
    let started = Instant::now();
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(width),
        DevicePx(HEIGHT),
    )));
    harness.settle(256);
    started.elapsed()
}

/// The median cost of one configure, in microseconds.
///
/// The widths cycle rather than repeat, because a configure that restates the size the window
/// already has is a configure the window is entitled to answer with nothing at all — and a
/// measurement made of those measures the check that refuses them.
pub(crate) fn configures(harness: &mut Harness<Runtime>) -> f64 {
    let width = |turn: usize| WIDTH - ((turn % 16) as f32) * 8.0;
    for turn in 0..WARMUP {
        configure(harness, width(turn));
    }
    let mut samples: Vec<f64> = (0..REPEATS)
        .map(|turn| configure(harness, width(turn + WARMUP)).as_secs_f64() * 1e6)
        .collect();
    configure(harness, WIDTH);
    median(&mut samples)
}

/// The median cost of one whole-document content change, in microseconds.
///
/// The same restyle, relayout and full repaint a configure causes, reached by a route that has
/// nothing to do with the window's extent: a class on the root that every row's colour depends on.
/// This is the same-run baseline the resize slope is stated against, so that what is compared
/// across machines is a ratio rather than a duration.
pub(crate) fn content_changes(
    harness: &mut Harness<Runtime>,
    warm: RwSignal<bool, LocalStorage>,
) -> f64 {
    let flip = |harness: &mut Harness<Runtime>| {
        let started = Instant::now();
        warm.set(!warm.get());
        harness.settle(256);
        started.elapsed()
    };
    for _ in 0..WARMUP {
        flip(harness);
    }
    let mut samples: Vec<f64> = (0..REPEATS)
        .map(|_| flip(harness).as_secs_f64() * 1e6)
        .collect();
    median(&mut samples)
}
