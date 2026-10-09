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

use support::{Harness, background, color};
use zgui_interned::CustomPropertyName;
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

/// A list of `rows` rows, each holding `cells` children, already through a frame.
fn table(rows: usize, cells: usize) -> (Harness, Vec<zgui_dom::NodeIndex>) {
    let mut harness = Harness::new();
    let column = harness.append(harness.root, "column");
    let mut nodes = Vec::new();
    for _ in 0..rows {
        let row = harness.append(column, "row");
        for _ in 0..cells {
            harness.append(row, "cell");
        }
        nodes.push(row);
    }
    harness.add_author("row { color: rgb(1, 2, 3) }");
    harness.frame();
    harness.retire_all();
    (harness, nodes)
}

/// A change to a property nothing inherits cascades the element and nothing below it.
///
/// The engine's own rule hands every child a recascade whenever anything about the element
/// changed, because its difference does not tell reset properties from inherited ones. A row's
/// background is nobody else's business.
#[test]
fn a_reset_only_change_on_every_row_recascades_no_cell() {
    let _guard = measuring();
    let (mut harness, rows) = table(300, 6);

    counter::reset();
    harness.edit(|edit| {
        for (index, &row) in rows.iter().enumerate() {
            edit.set_style_property(
                row,
                "background-color",
                Some(&format!("rgb({}, 0, 0)", index % 200)),
            );
        }
    });
    let pass = harness.frame();
    let frame = counter::snapshot();

    assert!(pass.styled >= rows.len());
    if COUNTERS_ENABLED {
        assert_eq!(
            frame.elements_recascaded + frame.elements_restyled,
            rows.len() as u64,
            "a background moving on every row cascaded {} elements, so the cells were cascaded too",
            frame.elements_recascaded + frame.elements_restyled
        );
    }
}

/// A change to an inherited property still reaches everything below the element.
#[test]
fn an_inherited_change_on_a_row_recascades_its_cells() {
    let _guard = measuring();
    let (mut harness, rows) = table(20, 6);

    counter::reset();
    harness.edit(|edit| {
        edit.set_style_property(rows[3], "color", Some("rgb(200, 0, 0)"));
    });
    harness.frame();
    let frame = counter::snapshot();

    if COUNTERS_ENABLED {
        assert_eq!(
            frame.elements_recascaded + frame.elements_restyled,
            7,
            "the row and its six cells owe a cascade, and {} elements took one",
            frame.elements_recascaded + frame.elements_restyled
        );
    }
}

/// A list of `rows` rows of `cells` cells under one author sheet, already through a frame.
fn themed(rows: usize, cells: usize, css: &str) -> (Harness, Vec<zgui_dom::NodeIndex>) {
    let mut harness = Harness::new();
    let column = harness.append(harness.root, "column");
    let mut nodes = Vec::new();
    for _ in 0..rows {
        let row = harness.append(column, "row");
        for _ in 0..cells {
            harness.append(row, "cell");
        }
        nodes.push(row);
    }
    harness.add_author(css);
    harness.frame();
    harness.retire_all();
    (harness, nodes)
}

fn set_custom(harness: &mut Harness, node: zgui_dom::NodeIndex, name: &str, value: &str) {
    harness.edit(|edit| {
        assert!(edit.set_custom_property(node, CustomPropertyName::new(name), Some(value)));
    });
}

fn first_cell(harness: &Harness, row: zgui_dom::NodeIndex) -> zgui_dom::NodeIndex {
    harness
        .document
        .store()
        .core(row)
        .first_child()
        .expect("the row has cells")
}

/// A custom property that changes cascades the elements whose declarations read it, and the
/// elements between them and the change take the new map into the style they have.
#[test]
fn a_custom_property_change_recascades_its_readers_only() {
    let _guard = measuring();
    let (mut harness, rows) = themed(
        500,
        6,
        "row { background-color: var(--accent, rgb(1, 1, 1)) }",
    );

    counter::reset();
    let root = harness.root;
    set_custom(&mut harness, root, "accent", "rgb(0, 9, 0)");
    harness.frame();
    let frame = counter::snapshot();

    assert_eq!(background(&harness, rows[499]), (0, 9, 0));
    if COUNTERS_ENABLED {
        assert_eq!(
            frame.elements_recascaded + frame.elements_restyled,
            501,
            "the root and five hundred reading rows owe a cascade, and {} elements took one",
            frame.elements_recascaded + frame.elements_restyled
        );
        assert_eq!(
            frame.custom_maps_refreshed, 1,
            "the column takes the map without a cascade; the cells read nothing of it and are \
             not visited at all"
        );
    }
}

/// A custom property nothing reads cascades the element it was set on and nothing else.
#[test]
fn an_unread_custom_property_cascades_one_element() {
    let _guard = measuring();
    let (mut harness, _rows) = themed(500, 6, "row { color: var(--accent, rgb(1, 1, 1)) }");

    counter::reset();
    let root = harness.root;
    set_custom(&mut harness, root, "spare", "1");
    harness.frame();
    let frame = counter::snapshot();

    if COUNTERS_ENABLED {
        assert_eq!(frame.elements_recascaded + frame.elements_restyled, 1);
    }
}

/// An element that declares a custom property from another is a reader, and what it declares
/// reaches its own readers.
#[test]
fn a_declarer_chain_reaches_the_readers_at_its_end() {
    let _guard = measuring();
    let (mut harness, rows) = themed(
        100,
        4,
        "row { --tone: var(--accent, rgb(1, 1, 1)) } cell { color: var(--tone) }",
    );

    counter::reset();
    let root = harness.root;
    set_custom(&mut harness, root, "accent", "rgb(0, 0, 9)");
    harness.frame();
    let frame = counter::snapshot();

    assert_eq!(color(&harness, first_cell(&harness, rows[50])), (0, 0, 9));
    if COUNTERS_ENABLED {
        assert_eq!(
            frame.elements_recascaded + frame.elements_restyled,
            1 + 100 + 400
        );
    }
}

/// A declarer that sets the changed name itself keeps the change from everything below it, and one
/// that declares an unrelated name passes the change on.
#[test]
fn a_declarer_passes_on_only_what_its_own_map_changed_by() {
    let _guard = measuring();
    let (mut harness, rows) = themed(
        100,
        4,
        "row { --spare: 1px } row.own { --accent: rgb(5, 5, 5) } \
         cell { color: var(--accent, rgb(1, 1, 1)) }",
    );
    for row in rows.iter().take(50) {
        harness.set_classes(*row, &["own"]);
    }
    harness.frame();

    counter::reset();
    let root = harness.root;
    set_custom(&mut harness, root, "accent", "rgb(0, 9, 0)");
    harness.frame();
    let frame = counter::snapshot();

    assert_eq!(color(&harness, first_cell(&harness, rows[10])), (5, 5, 5));
    assert_eq!(color(&harness, first_cell(&harness, rows[60])), (0, 9, 0));
    if COUNTERS_ENABLED {
        assert_eq!(
            frame.elements_recascaded + frame.elements_restyled,
            1 + 100 + 200,
            "the root, every declaring row, and the cells of the fifty rows that pass the change on"
        );
    }
}

/// An element that becomes a reader after the change cascades against the map as it is now.
///
/// Its parent took the map without a cascade, so the trap would be a cascade that reads the map
/// the parent's style was built against a frame ago.
#[test]
fn a_late_reader_cascades_against_the_current_map() {
    let _guard = measuring();
    let (mut harness, rows) = themed(
        50,
        4,
        "cell.reader { color: var(--accent, rgb(1, 1, 1)) } row.hi { background: rgb(7, 7, 7) }",
    );
    let cell = first_cell(&harness, rows[20]);

    counter::reset();
    let root = harness.root;
    set_custom(&mut harness, root, "accent", "rgb(9, 0, 0)");
    harness.frame();
    let frame = counter::snapshot();
    if COUNTERS_ENABLED {
        assert_eq!(frame.elements_recascaded + frame.elements_restyled, 1);
    }

    counter::reset();
    harness.set_classes(cell, &["reader"]);
    let pass = harness.frame();
    assert!(
        pass.traversed,
        "the class toggle owes a restyle whatever the last frame refreshed"
    );
    assert_eq!(color(&harness, cell), (9, 0, 0));

    counter::reset();
    harness.set_classes(rows[20], &["hi"]);
    harness.frame();
    let frame = counter::snapshot();
    assert_eq!(color(&harness, cell), (9, 0, 0));
    if COUNTERS_ENABLED {
        assert_eq!(
            frame.elements_recascaded + frame.elements_restyled,
            1,
            "a background on the row is nobody else's business"
        );
    }
}

/// A name the framework reads on an element's behalf counts every descendant as a reader.
#[test]
fn a_framework_read_name_recascades_everything_below() {
    let _guard = measuring();
    let (mut harness, _rows) = themed(50, 4, "row { color: rgb(1, 1, 1) }");

    counter::reset();
    let root = harness.root;
    set_custom(&mut harness, root, "zgui-fill", "rgb(9, 0, 0)");
    harness.frame();
    let frame = counter::snapshot();

    if COUNTERS_ENABLED {
        assert_eq!(
            frame.elements_recascaded + frame.elements_restyled,
            1 + 1 + 50 + 200
        );
        assert_eq!(frame.custom_maps_refreshed, 0);
    }
}

/// An element that declares an effect reads that effect's parameters by name, so any custom
/// property change reaches it as a cascade.
#[test]
fn an_effect_holder_recascades_on_any_custom_property_change() {
    let _guard = measuring();
    let (mut harness, rows) = themed(50, 4, "row.lit { --zgui-shader: glow }");
    harness.set_classes(rows[10], &["lit"]);
    harness.frame();
    harness.retire_all();

    counter::reset();
    let root = harness.root;
    set_custom(&mut harness, root, "glow-reach", "12");
    harness.frame();
    let frame = counter::snapshot();

    if COUNTERS_ENABLED {
        // The effect's name is a custom property, so the row's four cells inherit it and hold
        // the effect as much as the row does.
        assert_eq!(
            frame.elements_recascaded + frame.elements_restyled,
            1 + 1 + 4,
            "the root, the row holding the effect and the cells that inherit it"
        );
    }
}

/// A class toggled on one cell of a wide list starts the traversal at that cell.
#[test]
fn a_class_toggle_deep_in_a_wide_list_traverses_the_cell_and_nothing_above_it() {
    let _guard = measuring();
    let (mut harness, rows) = themed(500, 6, "cell.lit { color: rgb(9, 0, 0) }");
    let cell = first_cell(&harness, rows[250]);

    counter::reset();
    harness.set_classes(cell, &["lit"]);
    harness.frame();
    let frame = counter::snapshot();

    assert_eq!(color(&harness, cell), (9, 0, 0));
    if COUNTERS_ENABLED {
        assert_eq!(frame.elements_restyled, 1);
        assert!(
            frame.elements_traversed <= 2,
            "the traversal entered {} elements for one cell's class",
            frame.elements_traversed
        );
    }
}

/// A sibling combinator still fires when its subject's earlier sibling changes deep in the tree.
#[test]
fn a_sibling_rule_fires_when_the_traversal_starts_below_the_root() {
    let _guard = measuring();
    let (mut harness, rows) = themed(200, 4, "cell.a + cell { color: rgb(0, 9, 0) }");
    let first = first_cell(&harness, rows[100]);
    let second = harness
        .document
        .store()
        .core(first)
        .next_sibling()
        .expect("the row has a second cell");

    harness.set_classes(first, &["a"]);
    harness.frame();
    assert_eq!(color(&harness, second), (0, 9, 0));

    harness.set_classes(first, &[]);
    harness.frame();
    assert_ne!(color(&harness, second), (0, 9, 0));
}

/// Two mutations far apart keep the traversal above both of them, and both are styled.
#[test]
fn two_distant_mutations_are_both_styled() {
    let _guard = measuring();
    let (mut harness, rows) = themed(500, 6, "cell.lit { color: rgb(9, 0, 0) }");
    let near = first_cell(&harness, rows[10]);
    let far = first_cell(&harness, rows[490]);

    counter::reset();
    harness.set_classes(near, &["lit"]);
    harness.set_classes(far, &["lit"]);
    let pass = harness.frame();
    let frame = counter::snapshot();

    assert_eq!(color(&harness, near), (9, 0, 0));
    assert_eq!(color(&harness, far), (9, 0, 0));
    assert_eq!(pass.matched, 2);
    if COUNTERS_ENABLED {
        assert!(
            frame.elements_traversed <= 2 + 2 + 500 + 2,
            "the traversal entered {} elements for two cells' classes",
            frame.elements_traversed
        );
    }
}

/// A custom property change visits the subtrees that read it and skips the ones that do not.
#[test]
fn a_custom_property_change_skips_subtrees_that_read_nothing_of_it() {
    let _guard = measuring();
    let (mut harness, rows) = themed(
        500,
        6,
        "row { background-color: var(--accent, rgb(1, 1, 1)) }",
    );

    counter::reset();
    let root = harness.root;
    set_custom(&mut harness, root, "accent", "rgb(0, 9, 0)");
    harness.frame();
    let frame = counter::snapshot();

    assert_eq!(background(&harness, rows[499]), (0, 9, 0));
    if COUNTERS_ENABLED {
        assert_eq!(frame.elements_recascaded + frame.elements_restyled, 501);
        assert_eq!(
            frame.custom_subtrees_skipped, 3000,
            "traversed {} refreshed {} skipped {}",
            frame.elements_traversed, frame.custom_maps_refreshed, frame.custom_subtrees_skipped
        );
        assert!(
            frame.elements_traversed <= 502,
            "the traversal entered {} elements for five hundred readers",
            frame.elements_traversed
        );
    }
}

/// A reader that appears under a subtree the change skipped cascades against the current map.
///
/// The skipped subtree still holds the old map and the bit that says so; the traversal that
/// reaches the new reader passes through its parent first, which takes the map then, before the
/// reader's own cascade reads it.
#[test]
fn a_reader_inserted_under_a_skipped_subtree_reads_the_current_value() {
    let _guard = measuring();
    let (mut harness, rows) = themed(
        50,
        4,
        "row { background-color: var(--accent, rgb(1, 1, 1)) }
         cell.reader { color: var(--accent, rgb(1, 1, 1)) }",
    );
    let cell = first_cell(&harness, rows[20]);

    counter::reset();
    let root = harness.root;
    set_custom(&mut harness, root, "accent", "rgb(9, 0, 0)");
    harness.frame();
    let frame = counter::snapshot();
    if COUNTERS_ENABLED {
        assert_eq!(frame.custom_subtrees_skipped, 200, "every cell was skipped");
    }

    // The traversal starts at the cell, whose map is stale: the cell takes its parent's map
    // first and cascades against it.
    harness.set_classes(cell, &["reader"]);
    let pass = harness.frame();
    assert!(pass.traversed);
    assert_eq!(color(&harness, cell), (9, 0, 0));

    // A new element under a skipped cell: its first cascade reads the cell's map, which the
    // traversal refreshed on its way down.
    let deep = harness.append(first_cell(&harness, rows[30]), "cell");
    harness.add_author("cell cell { color: var(--accent, rgb(1, 1, 1)) }");
    harness.frame();
    assert_eq!(color(&harness, deep), (9, 0, 0));
}

/// A subtree whose only reader stopped reading is skipped by the next change.
#[test]
fn a_subtree_whose_reader_left_is_skipped_by_the_next_change() {
    let _guard = measuring();
    let (mut harness, rows) = themed(
        50,
        4,
        "row { background-color: var(--accent, rgb(1, 1, 1)) }
         cell.reader { color: var(--accent, rgb(1, 1, 1)) }",
    );
    let cell = first_cell(&harness, rows[20]);
    harness.set_classes(cell, &["reader"]);
    harness.frame();
    harness.retire_all();

    counter::reset();
    let root = harness.root;
    set_custom(&mut harness, root, "accent", "rgb(9, 0, 0)");
    harness.frame();
    let frame = counter::snapshot();
    assert_eq!(color(&harness, cell), (9, 0, 0));
    if COUNTERS_ENABLED {
        assert_eq!(
            frame.custom_subtrees_skipped, 199,
            "the reading cell was visited"
        );
    }

    harness.set_classes(cell, &[]);
    harness.frame();
    harness.retire_all();

    counter::reset();
    set_custom(&mut harness, root, "accent", "rgb(0, 9, 0)");
    harness.frame();
    let frame = counter::snapshot();
    if COUNTERS_ENABLED {
        assert_eq!(
            frame.custom_subtrees_skipped, 200,
            "the cell that stopped reading is skipped"
        );
    }
    assert_eq!(background(&harness, rows[20]), (0, 9, 0));
}

/// A font size that moved changes nothing an accessibility node states except its rectangle.
#[test]
fn a_font_size_change_marks_no_element_for_the_accessibility_phase() {
    let _guard = measuring();
    let (mut harness, rows) = themed(50, 4, "cell { white-space: nowrap; overflow: hidden }");
    let root = harness.root;
    harness.edit(|edit| {
        edit.set_style_property(root, "font-size", Some("20px"));
    });
    harness.frame();
    let mut marked = Vec::new();
    for row in &rows {
        if harness.owed(*row).contains(zgui_bits::Dirty::A11Y) {
            marked.push(("row", *row));
        }
        let cell = first_cell(&harness, *row);
        if harness.owed(cell).contains(zgui_bits::Dirty::A11Y) {
            marked.push(("cell", cell));
        }
    }
    assert!(
        marked.is_empty(),
        "{} elements owe the accessibility phase for a font size: {:?}",
        marked.len(),
        &marked[..marked.len().min(4)]
    );
}
