//! A hundred thousand rows under a fast wheel and under a touchpad.
//!
//! The second of the reference workloads, and the one the scroll half of the compositor programme
//! is argued against. The document is [`zgui_ui::virtualize::VirtualList`] — the component this
//! library ships, not a shape invented to be measured — at four model sizes and four port heights.
//!
//! ```text
//! cargo run --release -p zgui-bench --bin list-slope
//! ```
//!
//! # Two sweeps, because there are two different claims
//!
//! **The virtualisation sweep** holds the port at 800 CSS pixels and the row height at 24, so the
//! *same number of rows exist* at every size, and varies the model: 12 500, 25 000, 50 000,
//! 100 000. What it publishes is nanoseconds per **model row**, and the right answer is zero — a
//! list's length is data, and the cost of scrolling it should be a function of the space it is
//! shown in. The gate is the same-run ratio of the largest document's cost to the smallest's, taken
//! in one process minutes apart at most: a machine twice as fast halves both and leaves it exactly
//! where it was. It runs under both gestures, because a wheel and a touchpad reach the scroller by
//! different routes and only one of them is a glide.
//!
//! **The glide sweep** holds the model at 100 000 rows and varies the port — 200, 400, 800, 1 600
//! CSS pixels — so the number of rows that *exist* varies by a factor of eight while the data does
//! not. What it publishes is the **glide nanoseconds per realised box**: the recorded baseline the
//! deliverable names, and the number a phase claiming to make scrolling cheaper has to move. Its
//! same-run baseline is a full repaint of the same realised rows on the same four documents,
//! reached by a class flip that has nothing to do with scrolling.
//!
//! # And what a tick of it damages
//!
//! Separately from either sweep, at the reference configuration — 100 000 rows, an 800-pixel port —
//! both gestures are driven once with a renderer that records what every accepted frame damaged.
//! The damage fraction and `fragments_rebuilt` per drawn frame are dimensionless before anybody
//! divides them, which makes them the two numbers here that are the same on every machine and need
//! no baseline at all.

#![forbid(unsafe_code)]

mod criteria;
mod document;
mod gesture;
mod measure;

use zgui_bench::reference::{fit, verdict};

use crate::gesture::Gesture;

/// The model sizes the virtualisation sweep runs over, in rows.
const ROWS: [usize; 4] = [12_500, 25_000, 50_000, 100_000];

/// The port heights the glide sweep runs over, in CSS pixels.
///
/// Four rather than two because a slope taken from two points is a line through two points: it
/// cannot tell a cost proportional to the realised rows from one proportional to their square.
const PORTS: [f32; 4] = [200.0, 400.0, 800.0, 1_600.0];

/// The port the virtualisation sweep holds fixed, and the one the damage numbers are taken at.
const REFERENCE_PORT: f32 = 800.0;

/// The model size the glide sweep holds fixed, and the one the damage numbers are taken at.
const REFERENCE_ROWS: usize = 100_000;

/// Runs the virtualisation sweep for one gesture, returning its points.
fn virtualisation(gesture: Gesture) -> Vec<measure::Point> {
    ROWS.map(|rows| {
        let point = measure::at(rows, REFERENCE_PORT, gesture);
        println!(
            "VIRT {} rows={rows} port={REFERENCE_PORT} built={} cost_ns={:.0}",
            gesture.name(),
            point.built,
            point.cost,
        );
        point
    })
    .into_iter()
    .collect()
}

/// A sweep's (size, cost) points, ready for a least-squares fit.
type Points = Vec<(f64, f64)>;

/// Runs the glide sweep, returning the glide points and the repaint points beside them.
fn glide() -> (Points, Points) {
    let mut glides = Vec::new();
    let mut repaints = Vec::new();
    for height in PORTS {
        let mut open = document::opened(REFERENCE_ROWS, height);
        let built = document::rows_built(&open.harness);
        assert!(
            built > 0,
            "a {REFERENCE_ROWS}-row list in a {height}px port built no rows, so the glide sweep \
             is measuring an empty document",
        );
        let pass = measure::gesture_ns(&mut open, height, Gesture::Wheel);
        let Some(glide) = pass.per_frame() else {
            panic!(
                "a wheel glide in a {height}px port drew no frames at all, so there is no \
                 per-frame cost to fit a slope through",
            );
        };
        let repaint = measure::repaint_ns(&mut open);
        println!(
            "GLIDE port={height} built={built} frames={} glide_ns_per_frame={glide:.0} \
             repaint_ns={repaint:.0}",
            pass.frames,
        );
        #[expect(
            clippy::cast_precision_loss,
            reason = "a realised row count is in the tens, exactly representable"
        )]
        let axis = built as f64;
        glides.push((axis, glide));
        repaints.push((axis, repaint));
    }
    (glides, repaints)
}

fn main() {
    let mut report = verdict::Report::new();

    for gesture in Gesture::ALL {
        let points = virtualisation(gesture);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a model row count is a hundred thousand at the largest"
        )]
        let axis: Vec<(f64, f64)> = points
            .iter()
            .map(|point| (point.rows as f64, point.cost))
            .collect();
        let built: Vec<usize> = points.iter().map(|point| point.built).collect();
        assert!(
            built.iter().all(|count| *count == built[0]),
            "the virtualisation sweep built {built:?} rows at its four model sizes. It is supposed \
             to hold the realised set fixed and vary only the data, so a sweep whose realised sets \
             differ is measuring both axes at once and its ratio means nothing.",
        );
        if let Some(slope) = fit::slope(&axis) {
            report.advisory(format!(
                "{} slope {slope:.6} ns per model row, over {} rows realising {} of them",
                gesture.name(),
                ROWS[ROWS.len() - 1],
                built[0],
            ));
        }
        let first = points.first().map(|point| point.cost);
        let last = points.last().map(|point| point.cost);
        report.judged(&criteria::virtualisation(gesture).judge(last, first));

        let ticks = measure::per_tick(
            &mut document::opened(REFERENCE_ROWS, REFERENCE_PORT),
            REFERENCE_PORT,
            gesture,
        );
        println!(
            "TICK {} frames={} full={} damage={:?} fragments_rebuilt={:?}",
            gesture.name(),
            ticks.frames,
            ticks.full,
            ticks.damage,
            ticks.fragments_rebuilt,
        );
        report.judged(&criteria::damage(gesture).judge_directly(ticks.damage));
        report.judged(&criteria::FULL_FRAMES.judge_directly(ticks.full_frames));
        report.judged(&criteria::rebuilds(gesture).judge_directly(ticks.fragments_rebuilt));
    }

    let (glides, repaints) = glide();
    let glide_slope = fit::slope(&glides);
    let repaint_slope = fit::slope(&repaints);
    if let Some(slope) = glide_slope {
        report.advisory(format!(
            "glide slope {slope:.1} ns per realised box per drawn frame"
        ));
    }
    if let Some(slope) = repaint_slope {
        report.advisory(format!("repaint slope {slope:.1} ns per realised box"));
    }
    report.judged(&criteria::GLIDE.judge(glide_slope, repaint_slope));

    println!("{report}");
    if !report.passed() {
        std::process::exit(1);
    }
}
