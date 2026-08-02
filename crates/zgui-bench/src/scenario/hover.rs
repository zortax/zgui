//! Hover storm: a pointer dragged across a thousand-row table at 120 Hz.
//!
//! The interaction a framework is easiest to be wrong about. Crossing a row changes two elements —
//! the one entered and the one left — and everything downstream of that should be proportional to
//! two, not to a thousand: two restyles, two fragments rebuilt, a damaged region the size of two
//! rows, and a handful of primitives re-emitted inside it. A frame that emits the whole table is
//! still fast enough to look fine on this document and will not be on the next one, which is why
//! the primitive count is banded beside the time rather than instead of it.

use zgui::vocab::PointerAction;

use crate::scenario::band::{Band, INTERACTION_TOLERANCE, Measurement, Pace, Spread};
use crate::scenario::fixture::{self, Fixture};
use crate::scenario::{Outcome, counters, quiet};

/// How many rows the pointer crosses.
///
/// One pass down the visible rows and back up, twice, at the 120 Hz a high-refresh pointer
/// delivers: enough samples for a median that means something and enough passes that a cache which
/// only works the first time down is visible as a slower second pass.
const CROSSINGS: usize = 240;

/// Runs the scenario.
pub(crate) fn run() -> Outcome {
    let mut harness = crate::drive::harness(fixture::runtime(Fixture::Table));
    quiet(&mut harness);

    let rows = fixture::visible_table_rows();
    let mut samples = Vec::with_capacity(CROSSINGS);
    let mut frames = 0_u64;
    let mut emitted = 0_u64;
    let mut restyled = 0_u64;

    let before = zgui_profile::counter::snapshot();
    for step in 0..CROSSINGS {
        let index = step % rows;
        let at = fixture::table_row_centre(index);
        let mark = zgui_profile::counter::snapshot();
        let started = std::time::Instant::now();
        harness.deliver_to_first(crate::input::pointer(PointerAction::Moved, at));
        let ran = harness.settle(64);
        samples.push(started.elapsed().as_secs_f64() * 1e6);
        let moved = mark.delta(&zgui_profile::counter::snapshot());
        frames += ran;
        emitted += moved.primitives_emitted;
        restyled += moved.elements_restyled;
        // A pointer aimed at nothing costs nothing and would pass every band below. The rows are
        // placed by a declared height, so a sheet that stops applying puts every sample in empty
        // space — and a hover storm over empty space is the one result this scenario must not be
        // able to report as a pass.
        harness.advance(std::time::Duration::from_micros(8_333));
        frames += harness.pump();
    }
    let all = before.delta(&zgui_profile::counter::snapshot());
    assert!(
        restyled > 0,
        "no element was restyled by {CROSSINGS} pointer crossings, so the pointer never reached a \
         row and nothing below was measured"
    );

    #[expect(
        clippy::cast_precision_loss,
        reason = "frame counts here are in the hundreds"
    )]
    let per_frame = emitted as f64 / frames.max(1) as f64;
    let pace = Pace::of(&samples, 8_333.0);
    let crossing = Spread::of(&mut samples);

    Outcome {
        scenario: "hover-storm",
        document: format!(
            "{} rows built, {rows} on screen, {CROSSINGS} crossings at 120 Hz",
            fixture::TABLE_ROWS
        ),
        measurements: vec![
            Measurement {
                name: "hover.crossing",
                unit: "us",
                value: crossing.p50,
                band: Band::Time {
                    baseline: 190.0,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "measured at 187 us, well under the half-millisecond budget",
                budget: Some(500.0),
                spread: Some(crossing),
            },
            Measurement {
                name: "hover.primitives_emitted",
                unit: "prims",
                value: per_frame,
                band: Band::Count { ceiling: 200 },
                rationale: "two rows changed, so a frame that emits the table is the defect",
                budget: Some(200.0),
                spread: None,
            },
        ]
        .into_iter()
        .chain(crate::scenario::band::whole_document_reshape(&all))
        .collect(),
        counters: counters(&all),
        notes: Vec::new(),
        pace,
    }
}
