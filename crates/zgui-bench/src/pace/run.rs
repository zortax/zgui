//! Driving the scripted scroll, and publishing what it measured.

use zgui::vocab::PointerAction;

use crate::pace::load::{BUSY, Load};
use crate::pace::report::Report;

/// The reference output's refresh interval, in microseconds.
///
/// Overridden by `ZGUI_REFRESH_US`, because the figure is a property of the display a run happened
/// on and a number keyed to one output is not portable to another.
const REFRESH_US: f64 = 13_347.0;

/// How long a scroll runs for, in seconds, unless the caller says otherwise.
const SECONDS: f64 = 60.0;

/// How often the scroll changes direction, in seconds.
///
/// Alternating rather than one long travel, because a scroll in one direction runs out of document
/// and stops, and a scroll that has stopped is not the thing being measured.
const REVERSE_EVERY: f64 = 2.0;

/// What the run is driven over and what it is called.
pub(crate) struct Script {
    /// Which document size.
    pub(crate) size: String,
    /// How many seconds to drive.
    pub(crate) seconds: f64,
}

/// Runs the scripted scroll and prints the report.
///
/// Returns `false` when the run may not be published: a machine that was busy while it ran, or a
/// build whose counters are compiled out.
pub(crate) fn run(script: &Script) -> bool {
    let refresh_us = std::env::var("ZGUI_REFRESH_US")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(REFRESH_US);

    let Some(before) = Load::now() else {
        eprintln!(
            "PACING refused: this machine publishes no load average, and a pacing number taken \
             without one is a number about an unknown machine"
        );
        return false;
    };

    let mut harness = crate::drive::opened(&script.size);
    harness.settle(256);
    let (boxes, fragments) = crate::inspect::document(&harness.app().windows()[0]);
    let middle = zgui::geom::Point::new(
        zgui::geom::CssPx(crate::gallery::WIDTH / 2.0),
        zgui::geom::CssPx(crate::gallery::HEIGHT / 2.0),
    );
    harness.deliver_to_first(crate::input::pointer(PointerAction::Moved, middle));
    harness.settle(64);

    let mut intervals = Vec::new();
    let started = std::time::Instant::now();
    let mut last_notch = std::time::Duration::ZERO;
    while started.elapsed().as_secs_f64() < script.seconds {
        let at = started.elapsed();
        if (at - last_notch).as_secs_f64() >= 1.0 / 12.0 {
            let forward = (at.as_secs_f64() / REVERSE_EVERY).floor() as i64 % 2 == 0;
            harness.deliver_to_first(crate::input::wheel(
                middle,
                if forward { 4.0 } else { -4.0 },
            ));
            last_notch = at;
        }
        let frame = std::time::Instant::now();
        harness.advance(std::time::Duration::from_micros(refresh_us as u64));
        harness.pump();
        intervals.push(frame.elapsed().as_secs_f64() * 1e6);
    }

    let after = Load::now().unwrap_or(before);
    let load = Load { before, after };
    let report = Report::of(&intervals, refresh_us);
    print(
        &report,
        &intervals,
        refresh_us,
        load,
        (boxes, fragments),
        script,
    );
    if load.peak() > BUSY {
        eprintln!(
            "PACING refused: {} — a pacing number taken on a busy machine is a number about \
             whatever else was running",
            load.describe()
        );
        return false;
    }
    true
}

/// Prints the report in the shape a stored artifact keeps it in.
fn print(
    report: &Report,
    intervals: &[f64],
    refresh_us: f64,
    load: Load,
    document: (usize, usize),
    script: &Script,
) {
    let (boxes, fragments) = document;
    println!(
        "PACING document=gallery-{} boxes={boxes} fragments={fragments} seconds={:.1} \
         refresh_us={refresh_us:.3}",
        script.size, script.seconds
    );
    println!("PACING machine {}", load.describe());
    println!(
        "PACING intervals n={} p50={:.3} p95={:.3} p99={:.3} max={:.3} (ms)",
        report.intervals.samples,
        report.intervals.p50 / 1000.0,
        report.intervals.p95 / 1000.0,
        report.intervals.p99 / 1000.0,
        report.intervals.max / 1000.0,
    );
    println!(
        "PACING delivered late={} of {} ({:.2} %) missed_vsyncs={}",
        report.pace.late,
        report.pace.frames,
        report.pace.late_fraction() * 100.0,
        Report::missed_vsyncs(intervals, refresh_us),
    );
    println!(
        "PACING ramp first_second_p50={:.3} last_second_p50={:.3} ratio={:.3} ms",
        report.first.p50 / 1000.0,
        report.last.p50 / 1000.0,
        report.ramp(),
    );
}
