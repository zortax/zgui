//! The planted violation, and its complement.

use zgui_profile::{Counter, Counters};

use crate::growth::compare::grown;

/// A pair of samples in which `counter` gained `by` between them.
fn leaked(counter: Counter, by: u64) -> (Counters, Counters) {
    let early = Counters::from_fn(|each| if each == counter { 2_002 } else { 7 });
    let late = Counters::from_fn(|each| if each == counter { 2_002 + by } else { 7 });
    (early, late)
}

#[test]
fn growth_gate_fails_on_a_planted_leak() {
    // A fixture that interns a clip chain per tick and never releases it, in the shape the check
    // sees it: the count is larger at the late sample than at the early one.
    let (early, late) = leaked(Counter::ClipEntriesLive, 165_486);
    let found = grown(&early, &late);
    assert_eq!(found.len(), 1, "one count grew, and one was reported");
    assert_eq!(found[0].counter, Counter::ClipEntriesLive);
    assert_eq!(found[0].by(), 165_486);

    // And the other half, which is the one that makes the first half mean anything: with the leak
    // removed the same check passes.
    let (early, late) = leaked(Counter::ClipEntriesLive, 0);
    assert!(
        grown(&early, &late).is_empty(),
        "with nothing growing the gate has nothing to report"
    );
}

#[test]
fn a_count_that_shrank_is_not_growth() {
    let early = Counters::from_fn(|_| 900);
    let late = Counters::from_fn(|_| 100);
    assert!(
        grown(&early, &late).is_empty(),
        "a cache that gave something back is working, not leaking"
    );
}

#[test]
fn every_live_count_is_checked_and_no_total_is() {
    let names: Vec<&str> = Counter::live().map(Counter::name).collect();
    assert!(
        names.contains(&"clip_entries_live"),
        "the table the growth was measured on is in the set: {names:?}"
    );
    assert!(
        !names.contains(&"primitives_emitted"),
        "a running total is not a live count and must not be banded as one"
    );
    for counter in Counter::live() {
        let early = Counters::from_fn(|_| 1);
        let late = Counters::from_fn(|each| if each == counter { 2 } else { 1 });
        assert_eq!(
            grown(&early, &late).len(),
            1,
            "{} is watched by the check",
            counter.name()
        );
    }
}
