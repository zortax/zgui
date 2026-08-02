//! Keeping a ring of latency marks does not turn on the per-element ones.
//!
//! Two different measurements share one mechanism. A frame trace is a few dozen marks whatever the
//! document is; a per-element trace is one mark per restyled element, which on a real page is
//! thousands in a single frame. A bounded ring can hold the first and cannot hold the second — a
//! frame that overflows it leaves no frame boundary in it at all, so the trace the ring was kept
//! for is gone.
//!
//! Merely constructing an inspector retains a ring, and that must not be what decides it. So the
//! per-element marks have a switch of their own, and this asserts both halves of it: off by
//! default with a ring retained, and on when asked for — because a count band of zero that is zero
//! for the wrong reason would pass for ever.
//!
//! A target of its own because both the ring and the switch are process-global.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui_devtools::DevTools;

use support::{opened, run};

/// How many marks in the ring name an element rather than a stage of the frame.
fn per_element_marks() -> usize {
    zgui_profile::latency::recent()
        .iter()
        .filter(|mark| mark.stage.starts_with("why."))
        .count()
}

#[test]
fn retaining_a_latency_ring_does_not_enable_per_element_marks() {
    // Constructing this retains the ring, which is the condition under test.
    let tools = DevTools::new();
    let mut harness = opened(tools);
    assert!(
        zgui_profile::latency::retaining(),
        "the inspector did not retain a ring, so this asserts nothing"
    );
    harness.settle(256);

    // Opening the panel builds thirty-odd elements, every one of which is restyled for the first
    // time — the widest per-element mark there is.
    zgui_profile::latency::clear();
    tools.set_open(true);
    run(&mut harness, 8);
    assert_eq!(
        per_element_marks(),
        0,
        "a retained ring wrote per-element marks with nobody having asked for one"
    );

    // And the complement, so that a zero here can never be zero because nothing restyled.
    zgui_profile::latency::trace_elements(true);
    zgui_profile::latency::clear();
    tools.set_open(false);
    run(&mut harness, 8);
    tools.set_open(true);
    run(&mut harness, 8);
    let asked_for = per_element_marks();
    zgui_profile::latency::trace_elements(false);
    assert!(
        asked_for > 0,
        "the same restyle wrote no per-element marks even with the switch on, so the assertion \
         above passes whatever the switch does"
    );
}
