//! Tests for the phase taxonomy.

use std::collections::BTreeSet;

use super::Phase;

#[test]
fn every_stage_has_a_distinct_code_and_label() {
    let codes: BTreeSet<&str> = Phase::ALL.iter().map(|phase| phase.code()).collect();
    let labels: BTreeSet<&str> = Phase::ALL.iter().map(|phase| phase.label()).collect();
    assert_eq!(codes.len(), Phase::ALL.len(), "two stages share a code");
    assert_eq!(labels.len(), Phase::ALL.len(), "two stages share a label");
}

#[test]
fn the_list_is_in_execution_order() {
    let mut previous = None;
    for phase in Phase::ALL {
        if let Some(previous) = previous {
            assert!(previous < phase, "{previous:?} is listed after {phase:?}");
        }
        previous = Some(phase);
    }
}

/// Runs `body` with a subscriber that records at `debug`, since a span with nobody listening is
/// deliberately not built at all.
fn while_recording(body: impl FnOnce()) {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .finish();
    tracing::subscriber::with_default(subscriber, body);
}

#[test]
fn a_span_is_named_after_its_stage() {
    while_recording(|| {
        for phase in Phase::ALL {
            assert_eq!(
                phase.span().metadata().expect("a span has metadata").name(),
                phase.label()
            );
        }
    });
}

#[test]
fn the_frame_span_is_named_for_the_frame() {
    while_recording(|| {
        let span = super::frame(7);
        assert_eq!(
            span.metadata().expect("a span has metadata").name(),
            "frame"
        );
    });
}

#[test]
fn a_stage_span_nests_inside_the_frame_it_belongs_to() {
    while_recording(|| {
        let frame = super::frame(3);
        let _frame = frame.enter();
        let stage = Phase::Layout.span();
        assert!(!stage.is_disabled());
    });
}

#[test]
fn a_stage_displays_as_its_code_and_label() {
    assert_eq!(Phase::Restyle.to_string(), "P3 restyle");
    assert_eq!(Phase::Observe.to_string(), "P7.6 observe");
    assert_eq!(Phase::DamageExpand.to_string(), "P8a damage_expand");
}
