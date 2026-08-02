//! Idle: five thousand nodes, open, with nothing happening to any of them.
//!
//! Two questions, and the first is the one that matters. A window nothing is happening in must run
//! **no frames at all** — not cheap frames, none — because a framework that draws sixty times a
//! second over a still document is a framework that empties a battery for nothing, and no amount of
//! per-frame cheapness fixes it. That is a count, so it is a ceiling of zero.
//!
//! The second is what a turn of the loop costs when it decides *not* to draw. The loop still wakes:
//! the clock moves, timers are examined, the park is recomputed. Whatever that costs is paid
//! whenever anything else in the process wakes the thread, so it is worth a band of its own.
//!
//! # Why this is not the gallery
//!
//! The gallery is the document every other scenario runs on, and it cannot answer this one: it
//! ships an indeterminate progress bar, and a document with a running animation in it is a document
//! that is *supposed* to draw every refresh. Asking the frame-count question there would either
//! measure the animation or force the ceiling up to whatever the animation costs, and a ceiling
//! that admits sixty frames a second admits every regression this scenario exists to catch.

use crate::scenario::band::{Band, Measurement, Pace, Spread};
use crate::scenario::fixture::{self, Fixture};
use crate::scenario::{Outcome, counters, quiet};

/// The interval the loop is turned at, in microseconds.
const INTERVAL_US: f64 = 16_667.0;

/// How long the document is left alone, in refresh intervals.
///
/// Ten seconds at sixty hertz. The clock is the harness's, so this costs the run nothing in wall
/// time and there is no reason to ask a shorter question than the one that matters.
const TURNS: usize = 600;

/// How far above its baseline a turn of an idle loop may drift.
///
/// The only band here that is not written against a measurement it sits close to. A turn of a
/// parked loop over a still document is measured at well under a microsecond — it reads the clock,
/// finds no deadline due and returns — and a number that small is mostly the timer's own resolution,
/// so a forty per cent band around it would fail on the machine being busy for a moment. The
/// baseline is therefore rounded up to one microsecond and doubled, which leaves a limit two orders
/// of magnitude above the measurement and still catches the regression that matters: an idle turn
/// that started touching nodes lands in the tens of microseconds at five thousand of them, not at
/// two.
const IDLE_TURN_TOLERANCE: f64 = 1.0;

/// Runs the scenario.
pub(crate) fn run() -> Outcome {
    let mut harness = crate::drive::harness(fixture::runtime(Fixture::Still));
    quiet(&mut harness);
    let boxes = harness.app().windows()[0].layout().borrow().keys().len();

    let before = zgui_profile::counter::snapshot();
    let mut frames = 0;
    // Timed one turn at a time rather than as a total divided by a count. The mean of six hundred
    // turns cannot say whether one of them woke the whole document, and that is the only failure
    // this measurement has.
    let mut turns = Vec::with_capacity(TURNS);
    for _ in 0..TURNS {
        harness.advance(std::time::Duration::from_micros(16_667));
        let started = std::time::Instant::now();
        frames += harness.pump();
        turns.push(started.elapsed().as_secs_f64() * 1e6);
    }
    let moved = before.delta(&zgui_profile::counter::snapshot());
    let pace = Pace::of(&turns, INTERVAL_US);
    let turn = Spread::of(&mut turns);

    Outcome {
        scenario: "idle",
        document: format!(
            "{boxes} boxes over {} rows, {TURNS} turns with nothing touched",
            fixture::STILL_ROWS
        ),
        measurements: vec![
            Measurement {
                name: "idle.frames",
                unit: "frames",
                value: f64::from(u32::try_from(frames).unwrap_or(u32::MAX)),
                band: Band::Count { ceiling: 0 },
                rationale: "a still document draws nothing; one frame here is the whole defect",
                budget: Some(0.0),
                spread: None,
            },
            Measurement {
                name: "idle.turn",
                unit: "us",
                value: turn.p50,
                band: Band::Time {
                    baseline: 1.0,
                    tolerance: IDLE_TURN_TOLERANCE,
                },
                rationale: "a turn that draws nothing touches no node; measured at well under 1 us",
                budget: None,
                spread: Some(turn),
            },
        ]
        .into_iter()
        .chain(crate::scenario::band::whole_document_reshape(&moved))
        .collect(),
        counters: counters(&moved),
        notes: Vec::new(),
        pace,
    }
}
