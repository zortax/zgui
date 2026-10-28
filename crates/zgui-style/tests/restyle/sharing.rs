//! Elements far apart in a list that cascade to equal values share the groups of one style.

use style::properties::ComputedValues;
use style::selector_parser::PseudoElement;
use zgui_dom::NodeIndex;

use crate::support::{Harness, color, width};

/// How many row styles the list alternates between.
///
/// With six cells per row, the cells between two rows of one style are more than the engine's
/// own sharing cache holds, so that cache finds none of them.
const TINTS: usize = 8;

/// The cells of one row.
const CELLS: usize = 6;

/// The sheet of the list: a row style per tint, and a width per cell.
fn sheet(cell: &str) -> String {
    let mut css = String::new();
    for tint in 0..TINTS {
        css.push_str(&format!(
            ".t{tint} {{ background-color: rgb({tint}, 0, 0) }}\n"
        ));
    }
    for index in 0..CELLS {
        css.push_str(&format!(".k{index} {{ width: {}px }}\n", 10 * (index + 1)));
    }
    css.push_str(&format!(".c {{ {cell} }}\n"));
    css
}

/// A list of `rows` rows, each holding one cell per width class, already through a frame.
fn list(rows: usize, cell: &str) -> (Harness, Vec<Vec<NodeIndex>>) {
    let mut harness = Harness::new();
    harness.add_author(&sheet(cell));
    let column = harness.append(harness.root, "column");
    let mut cells = Vec::new();
    for row in 0..rows {
        let node = harness.append(column, "row");
        let tint = format!("t{}", row % TINTS);
        harness.set_classes(node, &[tint.as_str()]);
        let mut of_row = Vec::new();
        for index in 0..CELLS {
            let cell = harness.append(node, "text");
            let width = format!("k{index}");
            harness.set_classes(cell, &["c", width.as_str()]);
            of_row.push(cell);
        }
        cells.push(of_row);
    }
    harness.frame();
    harness.retire_all();
    (harness, cells)
}

/// The primary style of `node`.
fn style(harness: &Harness, node: NodeIndex) -> servo_arc::Arc<ComputedValues> {
    harness
        .document
        .node(node)
        .primary_style()
        .expect("the element is styled")
}

#[test]
fn cousins_with_equal_values_take_one_style() {
    let (harness, cells) = list(4 * TINTS, "padding: 0 6px");

    for index in 0..CELLS {
        let first = style(&harness, cells[0][index]);
        for row in &cells[1..] {
            assert!(
                servo_arc::Arc::ptr_eq(&first, &style(&harness, row[index])),
                "cell {index} of rows under different row styles kept a style of its own"
            );
        }
        assert_eq!(width(&harness, cells[3][index]), 10.0 * (index + 1) as f32);
    }
    assert!(
        !servo_arc::Arc::ptr_eq(&style(&harness, cells[0][0]), &style(&harness, cells[0][1])),
        "cells of different widths share a style"
    );
}

/// A cell that writes an inherited text property keeps its own text group.
///
/// Its text claims a brush slot against that group, and its shaped paragraph is keyed by the slot.
/// Sharing the group would give equal labels in boxes of different widths one shaped form.
#[test]
fn a_cell_that_writes_text_properties_keeps_its_own_text_group() {
    let (harness, cells) = list(4 * TINTS, "white-space: nowrap; padding: 0 6px");

    let first = style(&harness, cells[0][2]);
    let later = style(&harness, cells[TINTS + 3][2]);
    assert!(
        core::ptr::eq(first.get_position(), later.get_position())
            && core::ptr::eq(first.get_padding(), later.get_padding())
            && core::ptr::eq(first.get_box(), later.get_box()),
        "the two cells cascade to equal values and kept their own groups"
    );
    assert!(
        !core::ptr::eq(first.get_inherited_text(), later.get_inherited_text()),
        "the two cells share a text group, so they share a brush slot"
    );
    assert_eq!(
        first.get_inherited_text(),
        later.get_inherited_text(),
        "the text groups hold the same values"
    );
}

#[test]
fn every_selection_style_of_equal_cells_is_one_style() {
    let (harness, cells) = list(4 * TINTS, "white-space: nowrap");

    let selection = |node| {
        harness
            .document
            .node(node)
            .pseudo_style(&PseudoElement::Selection)
            .expect("the user-agent sheet styles every element's selection")
    };
    assert!(servo_arc::Arc::ptr_eq(
        &selection(cells[0][4]),
        &selection(cells[2 * TINTS + 5][4])
    ));
}

#[test]
fn an_element_with_an_inline_style_cascades_on_a_chain_of_its_own() {
    let (mut harness, cells) = list(4 * TINTS, "padding: 0 6px");

    let inline = cells[TINTS + 1][3];
    harness.edit(|edit| edit.set_inline_style(inline, Some("margin-left: 0px")));
    harness.frame();

    let own = style(&harness, inline);
    let other = style(&harness, cells[0][3]);
    assert!(!servo_arc::Arc::ptr_eq(&own, &other));
    assert!(
        !core::ptr::eq(own.get_position(), other.get_position()),
        "an element with an inline style took the groups of a kept style"
    );
}

#[test]
fn cells_whose_inherited_values_differ_keep_their_own_values() {
    let mut harness = Harness::new();
    harness.add_author(".small { font-size: 10px } .large { font-size: 20px } .c { width: 2em }");
    let column = harness.append(harness.root, "column");
    let mut cells = Vec::new();
    for row in 0..(4 * TINTS) {
        let node = harness.append(column, "row");
        harness.set_classes(node, &[if row % 2 == 0 { "small" } else { "large" }]);
        let cell = harness.append(node, "text");
        harness.set_classes(cell, &["c"]);
        cells.push(cell);
    }
    harness.frame();

    for (row, cell) in cells.iter().enumerate() {
        let expected = if row % 2 == 0 { 20.0 } else { 40.0 };
        assert_eq!(width(&harness, *cell), expected, "row {row}");
    }
    assert!(servo_arc::Arc::ptr_eq(
        &style(&harness, cells[0]),
        &style(&harness, cells[2 * TINTS])
    ));
}

/// Rows that cascade to equal values but differ in a class that a descendant rule reads.
///
/// The rows sit under two columns of different styles, so the engine's own caches cannot give
/// one row the style of the other, and the table gives it. The engine shares between cousins
/// when their parents hold one style, on the guarantee that every class up both parent chains is
/// the same. A row given a kept style whole breaks that guarantee, so its children must not take
/// a cousin's style.
#[test]
fn a_rule_that_reads_a_row_class_still_reaches_cells_under_rows_that_share_a_style() {
    let mut harness = Harness::new();
    harness.add_author(
        ".a { padding-left: 1px } .b { padding-left: 2px } .sel .c { color: rgb(255, 0, 0) }",
    );
    let mut cells = Vec::new();
    for (class, selected) in [("a", false), ("b", true), ("a", false), ("b", true)] {
        let column = harness.append(harness.root, "column");
        harness.set_classes(column, &[class]);
        for _ in 0..4 {
            let row = harness.append(column, "row");
            harness.set_classes(row, if selected { &["row", "sel"] } else { &["row"] });
            let cell = harness.append(row, "text");
            harness.set_classes(cell, &["c"]);
            cells.push((cell, selected));
        }
    }
    harness.frame();

    for (index, (cell, selected)) in cells.iter().enumerate() {
        let expected = if *selected { (255, 0, 0) } else { (0, 0, 0) };
        assert_eq!(color(&harness, *cell), expected, "cell {index}");
    }
}

/// Siblings that the engine's own cache gives one style keep one style as a root class flips.
///
/// Each cell writes its text colour, so each cascade makes a text group of its own. A kept style
/// from an earlier flip has another text group, and a sibling that compared itself with it would
/// get a new style of its own in place of the one its siblings share.
#[test]
fn siblings_keep_one_style_as_a_root_class_flips() {
    let mut harness = Harness::new();
    harness.add_author(".c { color: rgb(0, 0, 200) } .warm .c { padding-left: 1px }");
    let row = harness.append(harness.root, "row");
    let cells: Vec<NodeIndex> = (0..12)
        .map(|_| {
            let cell = harness.append(row, "text");
            harness.set_classes(cell, &["c"]);
            cell
        })
        .collect();
    harness.frame();
    harness.retire_all();

    let root = harness.root;
    for flip in 0..4 {
        harness.set_classes(root, if flip % 2 == 0 { &["warm"] } else { &[] });
        harness.frame();
        harness.retire_all();
        let first = style(&harness, cells[0]);
        for (index, cell) in cells.iter().enumerate() {
            assert!(
                servo_arc::Arc::ptr_eq(&first, &style(&harness, *cell)),
                "flip {flip}: cell {index} holds a style of its own"
            );
        }
    }
}
