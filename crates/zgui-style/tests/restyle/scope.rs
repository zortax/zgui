//! What a restyle narrows itself to: the marks it never turns into engine work.

use crate::support::{Harness, background, color};
use zgui_bits::Dirty;
use zgui_vocab::UiState;

#[test]
fn toggling_an_unstyled_state_bit_never_enters_the_style_engine() {
    let mut harness = Harness::new();
    let button = harness.append(harness.root, "control");
    harness.set_classes(button, &["btn"]);
    harness.add_author(".btn:hover { color: rgb(9, 0, 0) }");

    // Frame one. The sheet set changed, so the filters are switched off and every mutation takes
    // the full path — the assertion below deliberately does *not* hold here, and a case written
    // with only this frame in it would pass against a filter that never re-arms.
    let first = harness.frame();
    assert!(first.traversed);
    assert!(
        !harness.filter_is_disabled(),
        "the tail of the frame that changed the sheets is where the filters are rebuilt"
    );
    harness.retire_all();

    // Frame two, with the filters rebuilt from the rule set as it is now. Nothing styles
    // `:indeterminate`, so writing it cannot change a computed value.
    harness.set_state(button, UiState::INDETERMINATE, true);
    let second = harness.frame();
    assert_eq!(second.styled, 0, "the traversal did not run at all");
    assert_eq!(second.matched, 0);
    assert!(!second.traversed);
    assert!(
        !harness
            .owed(button)
            .intersects(Dirty::RESTYLE | Dirty::RECASCADE | Dirty::REPAINT | Dirty::RELAYOUT),
        "nothing downstream was asked for style, layout or paint work either: {:?}",
        harness.owed(button)
    );
    assert!(
        harness.owed(button).contains(Dirty::A11Y),
        "the one obligation a filtered state write still owes: whether the element is \
         indeterminate is part of what assistive technology is told, and that is not a \
         CSS-derived quantity"
    );

    // And the control: a bit something *does* style takes the full path on the same frame shape.
    harness.set_state(button, UiState::HOVER, true);
    let third = harness.frame();
    assert!(third.traversed);
    assert_eq!(color(&harness, button), (9, 0, 0));
}

#[test]
fn a_class_no_selector_mentions_is_written_without_entering_the_engine() {
    let mut harness = Harness::new();
    let row = harness.append(harness.root, "box");
    harness.add_author(".lit { color: rgb(9, 0, 0) }");
    harness.frame();
    harness.retire_all();

    harness.set_classes(row, &["data-variant-quiet"]);
    let pass = harness.frame();
    assert!(!pass.traversed, "no selector mentions that class");

    harness.set_classes(row, &["lit"]);
    let pass = harness.frame();
    assert!(pass.traversed);
    assert_eq!(color(&harness, row), (9, 0, 0));
}

#[test]
fn inserting_under_a_parent_with_no_structural_selector_restyles_one_element() {
    let mut harness = Harness::new();
    let list = harness.append(harness.root, "column");
    for _ in 0..8 {
        harness.append(list, "box");
    }
    harness.add_author("box { color: rgb(1, 1, 1) }");
    harness.frame();
    harness.retire_all();

    let added = harness.append(list, "box");
    let pass = harness.frame();

    assert_eq!(
        pass.styled_nodes(),
        vec![added],
        "no rule depends on the child list, so nothing else can have changed"
    );
    assert_eq!(pass.styled, 1);
    assert!(
        pass.matched < 8,
        "one element matched, and nothing else looked at the rule set: {}",
        pass.matched
    );
    assert_eq!(color(&harness, added), (1, 1, 1));
}

#[test]
fn prepending_a_row_to_a_striped_table_restyles_only_later_siblings() {
    /// How many rows the table has before the prepend.
    const ROWS: usize = 200;

    let mut harness = Harness::new();
    let table = harness.append(harness.root, "column");
    for _ in 0..ROWS {
        harness.append(table, "box");
    }
    harness.add_author(
        "box:nth-child(odd) { background-color: rgb(1, 0, 0) }\n\
         box:nth-child(even) { background-color: rgb(0, 1, 0) }",
    );
    harness.frame();
    harness.retire_all();

    let added = harness.prepend(table, "box");
    let pass = harness.frame();

    assert_eq!(
        pass.styled, 201,
        "the new row and every later sibling, whose stripe parity all moved"
    );
    assert!(pass.styled_nodes().contains(&added));

    // The oracle: a document built to the final shape and styled once. Only a document that never
    // held the stale value can see a missed sibling invalidation, because both halves of an
    // incremental comparison would share the same stale path.
    let mut oracle = Harness::new();
    let oracle_table = oracle.append(oracle.root, "column");
    let mut oracle_rows = Vec::new();
    for _ in 0..=ROWS {
        oracle_rows.push(oracle.append(oracle_table, "box"));
    }
    oracle.add_author(
        "box:nth-child(odd) { background-color: rgb(1, 0, 0) }\n\
         box:nth-child(even) { background-color: rgb(0, 1, 0) }",
    );
    oracle.frame();

    let mut incremental = Vec::new();
    let mut child = harness.document.store().core(table).first_child();
    while let Some(node) = child {
        incremental.push(background(&harness, node));
        child = harness.document.store().core(node).next_sibling();
    }
    let from_scratch: Vec<_> = oracle_rows
        .iter()
        .map(|node| background(&oracle, *node))
        .collect();
    assert_eq!(incremental, from_scratch);
}
