//! Scroll: a ten-thousand-row virtualised list carried past its port at 120 Hz.
//!
//! Measured as two bands, because scrolling a virtualised list is two different frames wearing the
//! same clothes. Most ticks move the content and nothing else — the same rows, at a new offset —
//! and such a frame owes no restyle, no layout and no rebuild of the hit index; it is a
//! *translation*. Every row-height of travel, the window moves by one and rows leave one end and
//! arrive at the other; that frame is a *recycle* and is allowed to cost more.
//!
//! Averaging the two hides the interesting one. A recycle frame that has become a translation
//! frame's cost is nothing to celebrate if the translation frame has quietly become a recycle.

use zgui::geom::{CssPx, Point};
use zgui::vocab::PointerAction;

use crate::scenario::band::{Band, INTERACTION_TOLERANCE, Measurement, Pace, Spread};
use crate::scenario::fixture::{self, Fixture};
use crate::scenario::{Outcome, counters, quiet};

/// How many ticks of the refresh the list is carried for.
const TICKS: usize = 600;

/// How many wheel notches are delivered along the way.
///
/// One every fortieth tick, so the glide never runs out and the list is genuinely in motion for the
/// whole of the measurement rather than decelerating through most of it.
const NOTCH_EVERY: usize = 40;

/// A total divided by how many frames it was collected over.
#[expect(
    clippy::cast_precision_loss,
    reason = "frame counts here are in the hundreds and counter totals in the hundreds of thousands"
)]
fn per(total: u64, frames: usize) -> f64 {
    total as f64 / frames.max(1) as f64
}

/// What one row shift costs in work rather than in time.
///
/// What a recycle frame's cost is read against, and the evidence a missed budget would be escalated
/// with. A window that moves by one row should build one row and drop one; anything much larger than
/// that is the port being rebuilt rather than retained, which is the question a time on its own can
/// never answer.
#[derive(Default)]
struct Rebuild {
    /// Boxes built across every recycle frame.
    boxes: u64,
    /// Nodes laid out again across every recycle frame.
    nodes: u64,
    /// Primitives emitted across every recycle frame.
    emitted: u64,
}

/// Runs the scenario.
pub(crate) fn run() -> Outcome {
    let mut harness = crate::drive::harness(fixture::runtime(Fixture::LongList));
    quiet(&mut harness);

    let middle = Point::new(
        CssPx(crate::gallery::WIDTH / 2.0),
        CssPx(crate::gallery::HEIGHT / 2.0),
    );
    harness.deliver_to_first(crate::input::pointer(PointerAction::Moved, middle));
    harness.settle(64);

    let mut translation = Vec::new();
    let mut recycle = Vec::new();
    let mut stray = crate::scenario::Stray::default();
    let mut rebuild = Rebuild::default();
    let before = zgui_profile::counter::snapshot();

    for tick in 0..TICKS {
        // A notch is not a tick of the glide, and the frames it produces are neither of the two
        // kinds this scenario is about: the discrete step it applies can carry the list past
        // several row boundaries at once, and the rebuild that answers it lands in whichever frame
        // the loop runs next. So the notch is delivered, settled and then left out of both buckets
        // — otherwise a handful of whole-list relayouts belonging to the notch are charged to the
        // translation frame that happened to follow it.
        if tick % NOTCH_EVERY == 0 {
            harness.deliver_to_first(crate::input::wheel(middle, 6.0));
            harness.settle(64);
            harness.advance(std::time::Duration::from_micros(8_333));
            harness.pump();
            continue;
        }
        let mark = zgui_profile::counter::snapshot();
        let started = std::time::Instant::now();
        harness.advance(std::time::Duration::from_micros(8_333));
        let ran = harness.pump();
        let cost = started.elapsed().as_secs_f64() * 1e6;
        if ran == 0 {
            continue;
        }
        let moved = mark.delta(&zgui_profile::counter::snapshot());
        // Which kind of frame this was, decided by whether the virtualised window moved: a frame
        // that built a row is the frame a row boundary fell in.
        //
        // This is not circular with the three claims below it. It would be if a translation frame
        // were defined as one that did *no* work, because then "a translation frame restyles
        // nothing" would be true by construction. What it is defined by is one counter — boxes
        // built — and what is claimed of it is three others: elements restyled, nodes laid out and
        // hit indices rebuilt. A frame that reused every box and restyled the document would land
        // in the translation bucket and be reported, which is exactly the result worth catching.
        if moved.boxes_rebuilt == 0 {
            translation.push(cost);
            stray.restyles += moved.elements_restyled;
            stray.relayouts += moved.nodes_relaid_out;
            stray.hit_index_rebuilds += moved.hit_index_rebuilds;
        } else {
            recycle.push(cost);
            rebuild.boxes += moved.boxes_rebuilt;
            rebuild.nodes += moved.nodes_relaid_out;
            rebuild.emitted += moved.primitives_emitted;
        }
    }
    let all = before.delta(&zgui_profile::counter::snapshot());
    assert!(
        !translation.is_empty() && !recycle.is_empty(),
        "a scroll that produced {} translation frames and {} recycle frames never carried the \
         list past a row boundary, so neither band was measured",
        translation.len(),
        recycle.len()
    );

    // Both kinds of frame together for the pacing figure, because what a display asks is whether
    // *this* interval got a frame, and it does not care which of the two kinds the frame was.
    let mut every: Vec<f64> = translation.iter().chain(recycle.iter()).copied().collect();
    every.sort_by(f64::total_cmp);
    let pace = Pace::of(&every, 8_333.0);
    let translated = Spread::of(&mut translation);
    let recycled = Spread::of(&mut recycle);

    Outcome {
        scenario: "scroll",
        document: format!(
            "{} rows, {} translation frames and {} recycle frames at 120 Hz",
            fixture::LIST_ROWS,
            translation.len(),
            recycle.len()
        ),
        measurements: vec![
            Measurement {
                name: "scroll.translation",
                unit: "us",
                value: translated.p50,
                band: Band::Time {
                    baseline: 250.0,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "measured at 217-250 us across runs; the band sits under the budget",
                budget: Some(1_000.0),
                spread: Some(translated),
            },
            Measurement {
                name: "scroll.recycle",
                unit: "us",
                value: recycled.p50,
                band: Band::Time {
                    baseline: 880.0,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "measured at 872-892 us across runs; the band sits under the budget",
                budget: Some(1_500.0),
                spread: Some(recycled),
            },
            Measurement {
                name: "scroll.translation.restyles",
                unit: "elems",
                value: f64::from(u32::try_from(stray.restyles).unwrap_or(u32::MAX)),
                band: Band::Count { ceiling: 0 },
                rationale: "moving content changes no computed style",
                budget: Some(0.0),
                spread: None,
            },
            Measurement {
                name: "scroll.translation.relayouts",
                unit: "nodes",
                value: f64::from(u32::try_from(stray.relayouts).unwrap_or(u32::MAX)),
                band: Band::Count { ceiling: 0 },
                rationale: "moving content changes no size",
                budget: Some(0.0),
                spread: None,
            },
            Measurement {
                name: "scroll.translation.hit_rebuilds",
                unit: "rebuilds",
                value: f64::from(u32::try_from(stray.hit_index_rebuilds).unwrap_or(u32::MAX)),
                band: Band::Count { ceiling: 0 },
                rationale: "the hit index is translated with the content, never rebuilt from it",
                budget: Some(0.0),
                spread: None,
            },
        ]
        .into_iter()
        .chain(crate::scenario::band::whole_document_reshape(&all))
        .collect(),
        counters: counters(&all),
        notes: vec![format!(
            "one row shift rebuilds {:.0} boxes and relays out {:.0} nodes, which is the row that \
             arrived and not the port it arrived in; it still emits {:.0} primitives, which is the \
             port, and is what a recycle frame's remaining cost is",
            per(rebuild.boxes, recycle.len()),
            per(rebuild.nodes, recycle.len()),
            per(rebuild.emitted, recycle.len()),
        )],
        pace,
    }
}
