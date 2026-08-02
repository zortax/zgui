//! Driving a thousand scroll ticks and reading the live counts either side of them.

use zgui::vocab::PointerAction;
use zgui_profile::Counters;

use crate::growth::compare::{Grew, grown};

/// How many ticks the early sample is taken after.
///
/// Late enough that the document has finished arriving at its working set — the first frames of a
/// scroll build rows that were never on screen, rasterise glyphs that were never drawn and intern
/// the paints those rows use — and early enough that whatever a leak gains per tick has had almost
/// no chance to accumulate. Anything a run holds at ten ticks it is entitled to hold at a thousand.
const EARLY: usize = 10;

/// How many ticks the late sample is taken after.
const LATE: usize = 1_000;

/// How often a wheel notch is delivered, in ticks.
const NOTCH_EVERY: usize = 8;

/// What the run found.
pub(crate) struct Outcome {
    /// The live counts after [`EARLY`] ticks.
    pub(crate) early: Counters,
    /// The live counts after [`LATE`] ticks.
    pub(crate) late: Counters,
    /// Every one of them that was larger at the second sample.
    pub(crate) grew: Vec<Grew>,
    /// Which document this was driven over.
    pub(crate) document: String,
}

/// Scrolls the gallery for [`LATE`] ticks and compares the live counts.
///
/// The direction alternates so that the port keeps recycling rows rather than running off the end
/// of the document and settling: a scroll that has stopped moving interns nothing and would report
/// a flat count for a document that leaks on every notch.
pub(crate) fn run(size: &str) -> Outcome {
    let mut harness = crate::drive::opened(size);
    harness.settle(256);
    let (boxes, fragments) = crate::inspect::document(&harness.app().windows()[0]);
    let middle = zgui::geom::Point::new(
        zgui::geom::CssPx(crate::gallery::WIDTH / 2.0),
        zgui::geom::CssPx(crate::gallery::HEIGHT / 2.0),
    );
    harness.deliver_to_first(crate::input::pointer(PointerAction::Moved, middle));
    harness.settle(64);

    let mut early = Counters::ZERO;
    for tick in 1..=LATE {
        if tick % NOTCH_EVERY == 0 {
            let lines = if tick % (NOTCH_EVERY * 20) == 0 {
                -4.0
            } else {
                4.0
            };
            harness.deliver_to_first(crate::input::wheel(middle, lines));
            harness.settle(64);
        }
        harness.advance(std::time::Duration::from_micros(8_333));
        harness.pump();
        if tick == EARLY {
            early = zgui_profile::counter::snapshot();
        }
    }
    let late = zgui_profile::counter::snapshot();

    Outcome {
        grew: grown(&early, &late),
        early,
        late,
        document: format!("gallery {size}, {boxes} boxes, {fragments} fragments"),
    }
}

/// Prints what the run found, and says whether it is a pass.
pub(crate) fn report(outcome: &Outcome) -> bool {
    println!("GROWTH document={}", outcome.document);
    println!(
        "GROWTH ticks early={EARLY} late={LATE} counters={}",
        zgui_profile::Counter::live().count()
    );
    for counter in zgui_profile::Counter::live() {
        let (early, late) = (outcome.early.get(counter), outcome.late.get(counter));
        println!(
            "GROWTH\t{}\t{early}\t{late}\t{}",
            counter.name(),
            if late > early { "GREW" } else { "flat" },
        );
    }
    if !zgui_profile::COUNTERS_ENABLED {
        println!("GROWTH inconclusive: the counters are compiled out of this build");
        return false;
    }
    for grew in &outcome.grew {
        eprintln!(
            "GROWTH VIOLATION {} was {} after {EARLY} ticks and {} after {LATE}: it gained {}, and \
             the band is zero",
            grew.counter.name(),
            grew.early,
            grew.late,
            grew.by(),
        );
    }
    outcome.grew.is_empty()
}
