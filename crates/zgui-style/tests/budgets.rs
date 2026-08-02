//! What a restyle costs, in counters rather than in seconds.
//!
//! A timing is a property of the machine; "toggling one class in a five-hundred-row list matches
//! one element against the rule set" is a property of the design, and it stays true on a slow
//! machine, a fast one and under a debugger.
//!
//! # Why this is a target of its own
//!
//! The counters are process-global. A case that reads one has to be the only thing bumping it, so
//! every counter assertion in this crate lives here, behind one lock, and no other target reads
//! them at all.

#[path = "support/mod.rs"]
mod support;

use std::sync::{Mutex, MutexGuard};

use support::Harness;
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};

/// Held for the whole of any case that reads a counter.
static COUNTERS: Mutex<()> = Mutex::new(());

/// Takes the counter lock and zeroes every counter.
fn measuring() -> MutexGuard<'static, ()> {
    let guard = COUNTERS.lock().unwrap_or_else(|held| held.into_inner());
    counter::reset();
    guard
}

/// A list of `rows` rows under the root, styled by one class rule, already through a frame.
fn list(rows: usize) -> (Harness, Vec<zgui_dom::NodeIndex>) {
    let mut harness = Harness::new();
    let column = harness.append(harness.root, "column");
    let mut nodes = Vec::new();
    for _ in 0..rows {
        nodes.push(harness.append(column, "box"));
    }
    harness.add_author(".lit { color: rgb(9, 0, 0) }");
    harness.frame();
    harness.retire_all();
    (harness, nodes)
}

#[test]
fn a_frame_with_no_input_at_all_moves_no_counter() {
    let _guard = measuring();
    let (mut harness, _rows) = list(200);

    counter::reset();
    let pass = harness.frame();
    let frame = counter::snapshot();

    assert!(!pass.traversed);
    if COUNTERS_ENABLED {
        assert_eq!(frame.elements_restyled, 0);
        assert_eq!(frame.elements_recascaded, 0);
        assert_eq!(
            frame.dirty_walk_steps, 0,
            "an idle frame does not even enter the retirement walk: the root's own word is the \
             whole-document skip and it works"
        );
    }
}

#[test]
fn toggling_one_class_matches_one_element_against_the_rule_set() {
    let _guard = measuring();
    let (mut harness, rows) = list(500);

    counter::reset();
    harness.set_classes(rows[250], &["lit"]);
    let pass = harness.frame();
    let frame = counter::snapshot();

    assert_eq!(pass.matched, 1);
    if COUNTERS_ENABLED {
        assert_eq!(
            frame.elements_restyled, 1,
            "five hundred rows, and exactly one of them was matched against the rule set"
        );
        assert_eq!(frame.elements_recascaded, 0);
        assert!(
            frame.dirty_walk_steps < 64,
            "the retirement walk descends by the dirty-child records rather than across every \
             child: {}",
            frame.dirty_walk_steps
        );
    }
}

#[test]
fn the_first_pass_over_a_document_matches_every_element_and_recascades_none() {
    let _guard = measuring();
    let mut harness = Harness::new();
    let column = harness.append(harness.root, "column");
    for _ in 0..100 {
        harness.append(column, "box");
    }
    harness.add_author("box { color: rgb(1, 1, 1) }");

    counter::reset();
    let pass = harness.frame();
    let frame = counter::snapshot();

    assert_eq!(pass.styled, harness.element_count());
    assert_eq!(
        pass.restyled, 0,
        "an element being styled for the first time is not being *re*styled, which is why both \
         numbers exist"
    );
    if COUNTERS_ENABLED {
        assert_eq!(frame.elements_restyled as usize, pass.matched);
        assert_eq!(
            frame.elements_recascaded, 0,
            "nothing was cascaded without being matched on a first pass"
        );
    }
}

#[test]
fn an_inherited_change_recascades_the_descendants_it_reaches_without_matching_them() {
    let _guard = measuring();
    let mut harness = Harness::new();
    let column = harness.append(harness.root, "column");
    let mut leaves = Vec::new();
    for _ in 0..50 {
        leaves.push(harness.append(column, "box"));
    }
    harness.add_author(".lit { color: rgb(9, 0, 0) }");
    harness.frame();
    harness.retire_all();

    counter::reset();
    // The colour is inherited, so every descendant's cascade has to run again — and none of them
    // has to be matched against the rule set, because no selector's answer changed for them.
    harness.set_classes(column, &["lit"]);
    let pass = harness.frame();
    let frame = counter::snapshot();

    assert_eq!(pass.matched, 1, "only the element whose classes changed");
    assert_eq!(
        pass.styled, 51,
        "and the fifty descendants that inherit from it"
    );
    if COUNTERS_ENABLED {
        assert_eq!(frame.elements_restyled, 1);
        assert_eq!(
            frame.elements_recascaded, 50,
            "a recascade is the cheap half, and counting it as a restyle would hide that"
        );
    }
}

#[test]
fn a_class_toggle_asks_the_matcher_about_one_element_and_a_first_pass_asks_about_all_of_them() {
    let _guard = measuring();
    let mut harness = Harness::new();
    let column = harness.append(harness.root, "column");
    let mut rows = Vec::new();
    for _ in 0..500 {
        rows.push(harness.append(column, "box"));
    }
    // Three rules, so a row is asked more than one question and the two numbers below cannot
    // coincide by accident.
    harness.add_author(
        "box { color: rgb(1, 1, 1) }\n\
         .lit { color: rgb(9, 0, 0) }\n\
         column > box:first-child { color: rgb(0, 9, 0) }",
    );

    // The control run. It is not decoration: `selector_matches` staying small on the toggle below
    // is the assertion, and an assertion that a counter is small is satisfied by a counter nothing
    // moves. This is the run that proves the counter moves, and by how much.
    counter::reset();
    harness.frame();
    let whole_document = counter::get(Counter::SelectorMatches);
    harness.retire_all();

    counter::reset();
    harness.set_classes(rows[250], &["lit"]);
    let pass = harness.frame();
    let one_element = counter::get(Counter::SelectorMatches);

    assert_eq!(
        pass.matched, 1,
        "one element was matched against the rule set"
    );
    if !COUNTERS_ENABLED {
        return;
    }
    assert!(
        whole_document > 500,
        "a first pass over five hundred rows has to ask the matcher at least one question per \
         row, and asked {whole_document}"
    );
    assert!(
        one_element > 0,
        "matching one element against a three-rule sheet asks the matcher something, so a zero \
         here means the count never reached the matching surface at all"
    );
    assert!(
        one_element * 50 < whole_document,
        "re-matching one row of five hundred asked {one_element} questions against the whole \
         document's {whole_document}: selector matching is being redone for rows nothing changed \
         about"
    );
}
