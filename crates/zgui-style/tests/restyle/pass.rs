//! The shape of one pass: what it collects, how wide it runs, and what it retires.

use crate::support::{Harness, color, radius};
use zgui_bits::Dirty;
use zgui_style::StylePool;

#[test]
fn a_root_font_size_change_reflows_every_rem_sized_descendant_in_one_frame() {
    let mut harness = Harness::new();
    let child = harness.append(harness.root, "box");
    harness.add_author("box { border-top-left-radius: 2rem }");
    harness.frame();
    assert_eq!(
        radius(&harness, child),
        32.0,
        "the root font size starts at the vocabulary's 16px"
    );
    harness.retire_all();

    harness.add_author("root { font-size: 20px }");
    let pass = harness.frame();

    assert_eq!(
        radius(&harness, child),
        40.0,
        "a `rem` resolves against the root's *computed* font size, which is only known after the \
         traversal — so without the fixpoint this is not stale, it is wrong"
    );
    assert_eq!(pass.passes, 2, "and it converged in exactly one extra pass");
}

#[test]
fn a_frame_whose_root_font_size_did_not_move_runs_one_pass() {
    let mut harness = Harness::new();
    harness.append(harness.root, "box");
    harness.add_author("box { color: rgb(1, 1, 1) }");
    let pass = harness.frame();
    assert_eq!(
        pass.passes, 1,
        "the fixpoint costs nothing for a document whose root metrics stand still"
    );
}

#[test]
fn the_restyled_set_is_collected_by_the_traversal_rather_than_read_back_off_the_tree() {
    let mut harness = Harness::new();
    let list = harness.append(harness.root, "column");
    let mut rows = Vec::new();
    for _ in 0..500 {
        rows.push(harness.append(list, "box"));
    }
    harness.add_author(".lit { color: rgb(9, 0, 0) }");
    let first = harness.frame();
    assert_eq!(
        first.styled,
        harness.element_count(),
        "the first pass styles everything, and reports none of it as *re*styled"
    );
    assert_eq!(first.restyled, 0);
    harness.retire_all();

    harness.set_classes(rows[250], &["lit"]);
    let pass = harness.frame();
    assert_eq!(
        pass.records.len(),
        1,
        "the report describes what the traversal touched; a report read back off the tree would \
         have five hundred entries to sift and would cost more than the restyle it describes"
    );
    assert_eq!(pass.restyled, 1);
    assert_eq!(pass.styled_nodes(), vec![rows[250]]);
}

#[test]
fn a_colour_change_is_reported_for_the_text_paint_table() {
    // The colour is declared on the ancestor and inherited, so the element's own text group is
    // whatever it was handed. An element that declares `color` itself is handed a freshly built
    // group by every cascade of it, which is a change the comparison correctly reports and which
    // would therefore hide the case below.
    let mut harness = Harness::new();
    let label = harness.append(harness.root, "label");
    harness.add_author("root { color: rgb(1, 0, 0) }\n.quiet { border-top-left-radius: 4px }");
    harness.frame();
    assert!(
        harness
            .engine
            .text_paint_updates()
            .iter()
            .any(|update| update.index == label),
        "a first style is a change from nothing, so it is reported too"
    );
    harness.retire_all();

    harness.replace(
        0,
        "root { color: rgb(2, 0, 0) }\n.quiet { border-top-left-radius: 4px }",
    );
    harness.frame();
    let updates = harness.engine.text_paint_updates();
    assert!(
        updates.iter().any(|update| update.index == label),
        "the colour moved, so the slot the shaped text resolves through has to be rewritten"
    );

    // And a restyle that leaves the colour alone reports nothing, or the list would be a list of
    // every restyled element and the indirection would buy nothing.
    //
    // The element restyled here *has* to be one the traversal really visited, and it has to
    // inherit its colour rather than declare it. A case whose element is filtered out before the
    // engine runs asserts only that a frame which styled nothing reported nothing; and an element
    // that declares `color` itself gets a fresh group from every cascade, which the identity
    // comparison correctly reports as a change.
    harness.retire_all();
    harness.set_classes(label, &["quiet"]);
    let pass = harness.frame();
    assert_eq!(
        pass.styled_nodes(),
        vec![label],
        "the traversal has to have visited it, or the assertion below is about an empty frame"
    );
    assert!(
        harness.engine.text_paint_updates().is_empty(),
        "nothing about the text's colour moved: {:?}",
        harness.engine.text_paint_updates()
    );
}

#[test]
fn parallelism_is_capped_at_six_workers() {
    let mut harness = Harness::new();
    let list = harness.append(harness.root, "column");
    for _ in 0..2000 {
        harness.append(list, "box");
    }
    harness.add_author("box { color: rgb(1, 1, 1) }");

    let six = StylePool::new(6);
    assert_eq!(six.width(), 6);
    let pass = harness.frame_on(&six);
    assert!(pass.styled > 2000);
    assert!(
        pass.workers > 1,
        "a pool that was handed over and never used would make the cap meaningless"
    );

    // Eight is not a slowdown. The engine's per-worker storage is a fixed-length array indexed by
    // the worker's index in the pool, so the seventh worker indexes past the end of it.
    let eight = StylePool::exactly(8);
    assert_eq!(eight.width(), 8);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut wide = Harness::new();
        let list = wide.append(wide.root, "column");
        for _ in 0..2000 {
            wide.append(list, "box");
        }
        wide.add_author("box { color: rgb(1, 1, 1) }");
        wide.frame_on(&eight)
    }));
    assert!(
        outcome.is_err(),
        "a pool wider than the engine supports has to fail loudly rather than quietly"
    );
}

#[test]
fn the_dependency_filters_are_rebuilt_from_the_rule_set_that_was_just_flushed() {
    let mut harness = Harness::new();
    assert!(
        harness.filter_is_disabled(),
        "a rule set with no flush behind it can prove nothing irrelevant"
    );

    harness.add_author(".btn:hover { color: rgb(9, 0, 0) }");
    assert!(
        harness.engine.disable_filters_if_sheets_changed(),
        "the frame in which the sheets changed takes the full path"
    );
    harness.engine.restyle(&mut harness.document, None);
    assert!(!harness.filter_is_disabled());
    let classes = harness.engine.dependencies().class_count();
    let attrs = harness.engine.dependencies().attr_count();
    assert_eq!(classes, 1, "one class is mentioned by one selector");

    // A second sheet disables them again, and the same frame's tail rebuilds them wider. The
    // counts are compared as growth rather than as absolutes, because the user-agent sheet names
    // attributes of its own and this is a statement about the rebuild, not about that sheet.
    harness.add_author(".card [data-state] { color: rgb(8, 0, 0) }");
    assert!(harness.engine.disable_filters_if_sheets_changed());
    harness.engine.restyle(&mut harness.document, None);
    assert_eq!(harness.engine.dependencies().class_count(), classes + 1);
    assert_eq!(harness.engine.dependencies().attr_count(), attrs + 1);
}

#[test]
fn the_restyle_retires_the_obligations_it_serviced_so_the_next_mark_is_not_dropped() {
    let mut harness = Harness::new();
    let node = harness.append(harness.root, "box");
    harness.add_author(".lit { color: rgb(9, 0, 0) }");
    harness.frame();

    assert!(
        !harness
            .owed_below(harness.root)
            .intersects(Dirty::RESTYLE | Dirty::RECASCADE),
        "nothing else drains these: the engine owns the traversal, so the restyle has to retire \
         them itself"
    );

    // The property that retirement exists for: a second mark on the same node has to reach the
    // traversal again. With the obligations left set, the mark returns at its own early-out, no
    // ancestor learns to descend, and the restyle is dropped silently.
    harness.set_classes(node, &["lit"]);
    let pass = harness.frame();
    assert_eq!(pass.styled_nodes(), vec![node]);
    assert_eq!(color(&harness, node), (9, 0, 0));

    harness.retire_all();
    harness.set_classes(node, &[]);
    let pass = harness.frame();
    assert_eq!(
        pass.styled_nodes(),
        vec![node],
        "and again, which is the frame where a lost retirement shows up"
    );
    assert_ne!(color(&harness, node), (9, 0, 0));
}
