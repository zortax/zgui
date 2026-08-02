//! What the non-vacuity assertion refuses, proved by handing it each refusable thing.

use crate::counter::store::{COUNTERS_ENABLED, add};
use crate::counter::table::Counter;
use crate::counter::{Group, non_vacuity::Scenario, non_vacuity::assert_non_vacuous};

/// The skip counter these cases are written on, and the counter of work it is read against.
const SKIPPED: Counter = Counter::ChunksTranslated;

/// What that counter's pair is, taken from the declaration rather than written out.
fn done() -> Counter {
    SKIPPED.group().done().expect("a declared skip")
}

/// A scenario that moves the skip counter by `skipped` and the work counter by `done`.
fn moving(described: &str, skipped: u64, performed: u64) -> Scenario<'_> {
    let pair = done();
    Scenario::new(described, move || {
        add(SKIPPED, skipped);
        add(pair, performed);
    })
}

#[test]
fn a_pair_that_fires_where_it_should_and_is_silent_where_it_should_not_passes() {
    assert_non_vacuous(
        SKIPPED,
        moving("a stage that reused eight answers", 8, 2),
        moving("a stage with nothing to reuse", 0, 10),
    );
}

#[test]
#[should_panic(expected = "which is the situation the skip exists for")]
fn a_skip_that_never_fires_is_refused() {
    // The whole failure this exists for: a counter that reads zero everywhere satisfies every
    // upper bound written about it, so the pass has to be the thing that is impossible.
    assert_non_vacuous(
        SKIPPED,
        moving("a stage that reused nothing after all", 0, 10),
        moving("a stage with nothing to reuse", 0, 10),
    );
}

#[test]
#[should_panic(expected = "where nothing may be reused")]
fn a_skip_that_fires_where_it_must_not_is_refused() {
    assert_non_vacuous(
        SKIPPED,
        moving("a stage that reused eight answers", 8, 2),
        moving("a stage that reused one it had no right to", 1, 10),
    );
}

#[test]
#[should_panic(expected = "would hold over a scenario that does nothing at all")]
fn a_silent_scenario_that_never_reached_the_stage_is_refused() {
    // The hole in "assert it stays at zero": an empty scenario satisfies it. So the silent half
    // has to show that the stage ran and chose not to skip, rather than that it never ran.
    assert_non_vacuous(
        SKIPPED,
        moving("a stage that reused eight answers", 8, 2),
        moving("a scenario that reaches nothing", 0, 0),
    );
}

#[test]
#[should_panic(expected = "is not declared as a skip")]
fn a_counter_that_is_not_a_skip_is_refused() {
    assert_non_vacuous(
        Counter::NodesVisited,
        moving("anything", 1, 1),
        moving("anything else", 0, 1),
    );
}

#[test]
fn every_declared_skip_names_a_distinct_counter_of_work_performed() {
    // A skip whose pair is itself, or whose pair is another skip, would be read against a number
    // with the same blind spot — and the assertion would be about two counters that can both be
    // zero for ever.
    for counter in Counter::ALL {
        let Some(pair) = counter.group().done() else {
            continue;
        };
        assert_ne!(counter, pair, "{} is its own pair", counter.name());
        assert!(
            matches!(pair.group(), Group::BackendNeutral),
            "{}'s pair `{}` is not a plain count of work performed",
            counter.name(),
            pair.name()
        );
    }
}

#[test]
fn the_declared_skips_are_exactly_the_ones_this_workspace_has_registered() {
    // Written out rather than derived, so that a new skip fails here and has to arrive with the
    // non-vacuity assertion `cargo xtask skips` will then demand of it.
    let skips: Vec<&str> = Counter::ALL
        .into_iter()
        .filter(|counter| counter.group().done().is_some())
        .map(Counter::name)
        .collect();
    assert_eq!(
        skips,
        vec![
            "layouts_held",
            "sizes_held",
            "place_writes_without_reemit",
            "chunks_translated",
            "primitives_culled",
            "sprites_resolved_at_push"
        ]
    );
}

#[test]
fn the_block_being_compiled_out_is_reported_rather_than_asserted_around() {
    // Nothing to prove when the counters are live; when they are not, the pass above was a pass
    // over four zeroes and this is the only thing saying so.
    if !COUNTERS_ENABLED {
        assert_non_vacuous(
            SKIPPED,
            moving("a stage that reused nothing at all", 0, 0),
            moving("the same", 0, 0),
        );
    }
}
