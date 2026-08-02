//! The Timeline tab does not write the marks it draws.
//!
//! It used to, and that was the lag. The strip drew one slice and up to two rows per latency mark;
//! the style engine wrote a mark per restyled element; the panel's own rows are elements. Each turn
//! of that loop the panel was bigger, so more elements restyled, so more marks were written, so the
//! next turn it was bigger again — reaching tens of thousands of nodes and frames of most of a
//! second on a page of forty rows. Its only brake was the ring overflowing, at which point no frame
//! boundary survived in it, the strip collapsed to its fallback and the whole thing began again.
//! That sawtooth is what "very laggy" was.
//!
//! So the loop is asserted directly, at the place it closes: what the window writes into the ring
//! with the tab open and idle is what it writes with the panel shut. A reader that feeds its own
//! writer cannot pass this, however well bounded each individual step of it is.
//!
//! A target of its own because the ring is process-global: a second test opening a second window
//! in the same binary would be writing into the counts this one is taking.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui_devtools::{DevTools, Tab};

use support::{boxes, opened, run};

/// How many marks the two runs may differ by and still be the same answer.
///
/// Not zero: an idle window is allowed to write a handful — a timer, a wake that found nothing to
/// do — and what this test is about is three orders of magnitude, not three marks.
const TOLERANCE: usize = 64;

/// How many turns each measurement runs for. Five seconds at 60 Hz.
const TURNS: usize = 300;

/// The panel open on the Timeline tab writes no more marks than the panel shut.
#[test]
fn the_timeline_does_not_write_the_marks_it_draws() {
    let tools = DevTools::new();
    let mut harness = opened(tools);

    // Shut, and idle. The baseline.
    harness.settle(256);
    zgui_profile::latency::clear();
    run(&mut harness, TURNS);
    let shut = zgui_profile::latency::recent().len();

    // Open on the tab that used to run away, given long enough to settle first.
    tools.set_open(true);
    tools.show(Tab::Timeline);
    run(&mut harness, 120);
    let settled = boxes(&harness);
    zgui_profile::latency::clear();
    run(&mut harness, TURNS);
    let open = zgui_profile::latency::recent().len();
    let grown = boxes(&harness);

    assert!(
        open <= shut + TOLERANCE,
        "the panel wrote {open} marks over {TURNS} idle vsyncs where the shut window wrote {shut}"
    );
    assert_eq!(
        grown, settled,
        "the document grew from {settled} boxes to {grown} while nothing but the panel was drawn"
    );
}
