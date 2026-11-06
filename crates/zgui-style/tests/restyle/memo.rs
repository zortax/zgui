//! Elements a restyle cascades together, where an earlier element's result can answer a later one.
//!
//! Each case holds two elements with one rule chain whose cascades read something different, in the
//! order that gives the second the first one's result if the difference is lost. They hold in every
//! mode of the memo, so a shadow run proves them against the cascade as well.

use servo_arc::Arc;
use style::computed_values::display::T as Display;
use style::properties::ComputedValues;
use style::selector_parser::PseudoElement;
use zgui_dom::NodeIndex;
use zgui_interned::CustomPropertyName;

use crate::support::{Harness, color, display, width};

/// An element called `name`, in `classes`, as the last child of `parent`.
fn child(harness: &mut Harness, parent: NodeIndex, name: &str, classes: &[&str]) -> NodeIndex {
    let node = harness.append(parent, name);
    harness.set_classes(node, classes);
    node
}

/// Sets the custom property `name` on `node`.
fn set_custom(harness: &mut Harness, node: NodeIndex, name: &str, value: &str) {
    harness.edit(|edit| {
        assert!(edit.set_custom_property(node, CustomPropertyName::new(name), Some(value)));
    });
}

/// The primary style of `node`.
fn style(harness: &Harness, node: NodeIndex) -> Arc<ComputedValues> {
    harness
        .document
        .node(node)
        .primary_style()
        .expect("the element is styled")
}

/// Takes the document through a frame, then moves `--x` on the root so that every reader cascades.
fn recascade(harness: &mut Harness) {
    harness.frame();
    harness.retire_all();
    let root = harness.root;
    set_custom(harness, root, "x", "rgb(0, 9, 0)");
    harness.frame();
}

#[test]
fn price_cells_take_the_new_colour() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(".price { color: var(--x, rgb(1, 1, 1)) }");
    let column = harness.append(root, "column");
    let mut prices = Vec::new();
    for _ in 0..40 {
        let row = harness.append(column, "row");
        prices.push(child(&mut harness, row, "text", &["price"]));
    }

    recascade(&mut harness);
    set_custom(&mut harness, root, "x", "rgb(0, 0, 9)");
    harness.frame();

    for price in prices {
        assert_eq!(color(&harness, price), (0, 0, 9));
    }
}

#[test]
fn a_flex_item_and_a_block_child_stay_apart() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".block { display: block }
         .flex { display: flex }
         .c { display: inline; color: var(--x, rgb(1, 1, 1)) }",
    );
    let block = child(&mut harness, root, "box", &["block"]);
    let flex = child(&mut harness, root, "box", &["flex"]);
    let inline = child(&mut harness, block, "box", &["c"]);
    let item = child(&mut harness, flex, "box", &["c"]);

    recascade(&mut harness);

    assert_eq!(display(&harness, inline), Display::Inline);
    assert_eq!(display(&harness, item), Display::Block);
}

#[test]
fn a_reset_property_taken_by_inherit_never_answers_another_element() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".wide { width: 20px }
         .narrow { width: 10px }
         .c { width: inherit; color: var(--x, rgb(1, 1, 1)) }",
    );
    let wide = child(&mut harness, root, "box", &["wide"]);
    let narrow = child(&mut harness, root, "box", &["narrow"]);
    let first = child(&mut harness, wide, "box", &["c"]);
    let second = child(&mut harness, narrow, "box", &["c"]);

    recascade(&mut harness);

    assert_eq!(width(&harness, first), 20.0);
    assert_eq!(width(&harness, second), 10.0);
}

#[test]
fn parents_with_different_custom_maps_stay_apart() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".red { --tint: rgb(9, 0, 0) }
         .blue { --tint: rgb(0, 0, 9) }
         .c { color: var(--tint); background-color: var(--x, rgb(1, 1, 1)) }",
    );
    let red = child(&mut harness, root, "box", &["red"]);
    let blue = child(&mut harness, root, "box", &["blue"]);
    let first = child(&mut harness, red, "box", &["c"]);
    let second = child(&mut harness, blue, "box", &["c"]);

    recascade(&mut harness);

    assert_eq!(color(&harness, first), (9, 0, 0));
    assert_eq!(color(&harness, second), (0, 0, 9));
}

#[test]
fn parents_with_different_text_groups_stay_apart() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".red { color: rgb(9, 0, 0) }
         .blue { color: rgb(0, 0, 9) }
         .c { background-color: var(--x, rgb(1, 1, 1)) }",
    );
    let red = child(&mut harness, root, "box", &["red"]);
    let blue = child(&mut harness, root, "box", &["blue"]);
    let first = child(&mut harness, red, "box", &["c"]);
    let second = child(&mut harness, blue, "box", &["c"]);

    recascade(&mut harness);

    assert_eq!(color(&harness, first), (9, 0, 0));
    assert_eq!(color(&harness, second), (0, 0, 9));
}

/// A child of a `display: contents` element takes its layout parent from further up, and that
/// parent's `display` decides whether the child is blockified.
#[test]
fn a_child_of_a_contents_parent_reads_its_layout_parent() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".block { display: block }
         .flex { display: flex }
         .row { display: contents; color: var(--x, rgb(1, 1, 1)) }
         .c { display: inline; color: var(--x, rgb(1, 1, 1)) }",
    );
    let block = child(&mut harness, root, "box", &["block"]);
    let flex = child(&mut harness, root, "box", &["flex"]);
    let first_row = child(&mut harness, block, "box", &["row"]);
    let second_row = child(&mut harness, flex, "box", &["row"]);
    let inline = child(&mut harness, first_row, "box", &["c"]);
    let item = child(&mut harness, second_row, "box", &["c"]);

    recascade(&mut harness);

    assert_eq!(display(&harness, inline), Display::Inline);
    assert_eq!(display(&harness, item), Display::Block);
}

/// Equal labels keep a text group each, because a shaped paragraph is keyed by the brush slot the
/// group claims, and a shaped form holds the lines of one width. The engine's own sharing covers
/// siblings under one parent, so these are cousins.
#[test]
fn equal_labels_keep_a_text_group_each() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".even { background-color: rgb(1, 0, 0) }
         .odd { background-color: rgb(2, 0, 0) }
         .label { color: var(--x, rgb(1, 1, 1)) }",
    );
    let even = child(&mut harness, root, "box", &["even"]);
    let odd = child(&mut harness, root, "box", &["odd"]);
    let first = child(&mut harness, even, "text", &["label"]);
    let second = child(&mut harness, odd, "text", &["label"]);

    recascade(&mut harness);

    let (first, second) = (style(&harness, first), style(&harness, second));
    assert_eq!(first.get_inherited_text(), second.get_inherited_text());
    assert!(!core::ptr::eq(
        first.get_inherited_text(),
        second.get_inherited_text()
    ));
}

/// Generated content lays out text of its own, so it keeps a text group of its own.
#[test]
fn generated_content_keeps_a_text_group_each() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".even { background-color: rgb(1, 0, 0) }
         .odd { background-color: rgb(2, 0, 0) }
         .c::before { content: \"x\"; color: rgb(9, 0, 0) }
         .c { background-color: var(--x, rgb(1, 1, 1)) }",
    );
    let even = child(&mut harness, root, "box", &["even"]);
    let odd = child(&mut harness, root, "box", &["odd"]);
    let first = child(&mut harness, even, "box", &["c"]);
    let second = child(&mut harness, odd, "box", &["c"]);

    recascade(&mut harness);

    let before = |node| {
        harness
            .document
            .node(node)
            .pseudo_style(&PseudoElement::Before)
            .expect("the element generates content")
    };
    let (first, second) = (before(first), before(second));
    assert_eq!(first.get_inherited_text(), second.get_inherited_text());
    assert!(!core::ptr::eq(
        first.get_inherited_text(),
        second.get_inherited_text()
    ));
}

/// Two elements can hold one primary chain and different `::selection` chains.
#[test]
fn elements_with_different_selection_rules_stay_apart() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".c { background-color: var(--x, rgb(1, 1, 1)) }
         .a::selection { background-color: rgb(9, 0, 0) }
         .b::selection { background-color: rgb(0, 0, 9) }",
    );
    let even = child(&mut harness, root, "box", &["even"]);
    let odd = child(&mut harness, root, "box", &["odd"]);
    let first = child(&mut harness, even, "box", &["c", "a"]);
    let second = child(&mut harness, odd, "box", &["c", "b"]);

    recascade(&mut harness);

    let selection = |node| {
        harness
            .document
            .node(node)
            .pseudo_style(&PseudoElement::Selection)
            .expect("the sheet styles the selection")
            .clone_background_color()
    };
    assert_ne!(selection(first), selection(second));
}

/// A size in `em` reads the parent's font group.
#[test]
fn parents_with_different_fonts_stay_apart() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".big { font-size: 20px }
         .small { font-size: 10px }
         .c { width: 2em; color: var(--x, rgb(1, 1, 1)) }",
    );
    let big = child(&mut harness, root, "box", &["big"]);
    let small = child(&mut harness, root, "box", &["small"]);
    let first = child(&mut harness, big, "box", &["c"]);
    let second = child(&mut harness, small, "box", &["c"]);

    recascade(&mut harness);

    assert_eq!(width(&harness, first), 40.0);
    assert_eq!(width(&harness, second), 20.0);
}

/// Two `display: contents` parents that are equal in every way a key sees can stand in layout
/// parents of different writing modes, and an inline child across that change is adjusted.
#[test]
fn contents_parents_over_different_writing_modes_stay_apart() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".block { display: block }
         .vertical { display: block; writing-mode: vertical-rl }
         .row { display: contents; writing-mode: horizontal-tb;
                color: var(--x, rgb(1, 1, 1)) }
         .c { display: inline; background-color: var(--x, rgb(1, 1, 1)) }",
    );
    let block = child(&mut harness, root, "box", &["block"]);
    let vertical = child(&mut harness, root, "box", &["vertical"]);
    let first_row = child(&mut harness, block, "box", &["row"]);
    let second_row = child(&mut harness, vertical, "box", &["row"]);
    let inline = child(&mut harness, first_row, "box", &["c"]);
    let turned = child(&mut harness, second_row, "box", &["c"]);

    harness.frame();
    assert_eq!(display(&harness, inline), Display::Inline);
    assert_eq!(
        display(&harness, turned),
        Display::InlineBlock,
        "this case needs the engine to adjust across the writing mode"
    );

    recascade(&mut harness);

    assert_eq!(display(&harness, inline), Display::Inline);
    assert_eq!(display(&harness, turned), Display::InlineBlock);
}

/// The selection colours read the element's custom properties through `::selection`.
#[test]
fn a_selection_style_follows_the_element() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".red { --zgui-selection: rgb(9, 0, 0) }
         .blue { --zgui-selection: rgb(0, 0, 9) }
         .c { background-color: var(--x, rgb(1, 1, 1)) }",
    );
    let red = child(&mut harness, root, "box", &["red"]);
    let blue = child(&mut harness, root, "box", &["blue"]);
    let first = child(&mut harness, red, "box", &["c"]);
    let second = child(&mut harness, blue, "box", &["c"]);

    recascade(&mut harness);

    let selection = |node| {
        harness
            .document
            .node(node)
            .pseudo_style(&PseudoElement::Selection)
            .expect("the sheet styles the selection")
            .clone_background_color()
    };
    assert_ne!(selection(first), selection(second));
}

/// An element that declares nothing of the text group holds its parent's, as a cascade leaves it.
#[test]
fn an_element_without_text_declarations_holds_its_parents_text_group() {
    let mut harness = Harness::new();
    let root = harness.root;
    harness.add_author(
        ".tint { color: var(--x, rgb(1, 1, 1)) }
         .c { background-color: var(--x, rgb(1, 1, 1)) }",
    );
    let first_parent = child(&mut harness, root, "box", &["tint"]);
    let second_parent = child(&mut harness, root, "box", &["tint"]);
    let first = child(&mut harness, first_parent, "box", &["c"]);
    let second = child(&mut harness, second_parent, "box", &["c"]);

    recascade(&mut harness);

    for (parent, node) in [(first_parent, first), (second_parent, second)] {
        assert!(core::ptr::eq(
            style(&harness, node).get_inherited_text(),
            style(&harness, parent).get_inherited_text(),
        ));
    }
}
