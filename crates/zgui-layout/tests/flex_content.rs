//! A flex item is sized by what is inside it.
//!
//! `flex-basis: auto` on an item whose `width` is `auto` means "as wide as your content wants to
//! be", which for an item holding text is the text. A flex container that does not ask its items
//! that question lays every one of them out at nothing, and a row of a drawing beside a label
//! becomes a drawing beside an empty box — the label is in the document, is styled, is announced to
//! a reader, and occupies no space at all.

mod support;

use support::{Element, Fixture, lay_out, measurer};
use zgui_layout::tree::store::LayoutStore;

/// The width of the box carrying `class`.
fn width_of(store: &LayoutStore, at: usize) -> f32 {
    nth(store, at)
        .and_then(|key| store.layout_of(key))
        .map(|layout| layout.border_box().size.width.0)
        .unwrap_or_else(|| panic!("no box at {at}"))
}

/// The key of the nth box in tree order, which the callers name by position.
fn nth(store: &LayoutStore, at: usize) -> Option<zgui_dom::side::BoxKey> {
    let mut out = Vec::new();
    let mut stack = vec![store.root()?];
    while let Some(key) = stack.pop() {
        out.push(key);
        let mut children = store.node(key).children.to_vec();
        children.reverse();
        stack.extend(children);
    }
    out.get(at).copied()
}

/// A block flex item holding text is as wide as the text.
#[test]
fn a_block_item_in_a_flex_row_is_as_wide_as_its_text() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("item").classes(&["label"]).text("hello")]),
        "root { display: flex; flex-direction: row; width: 400px }
         item { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 200.0);

    let width = width_of(&store, 1);
    assert!(
        width > 0.0,
        "a flex item holding `hello` laid out {width}px wide, so its text takes no room at all"
    );
}

/// And a flex container is as wide as the items inside it.
#[test]
fn a_flex_container_is_as_wide_as_what_it_holds() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("tab").classes(&["tab"]).children(vec![
                Element::new("mark").classes(&["mark"]),
                Element::new("item").classes(&["label"]).text("Elements"),
            ]),
        ]),
        "root { display: flex; flex-direction: row; width: 400px; align-items: flex-start }
         tab { display: flex; flex-direction: row; flex-grow: 0; flex-shrink: 0 }
         mark { display: block; width: 13px; height: 13px }
         item { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 200.0);

    let label = width_of(&store, 3);
    let tab = width_of(&store, 1);
    assert!(
        label > 0.0,
        "the label inside a nested flex row laid out {label}px wide"
    );
    assert!(
        tab >= label + 13.0,
        "the tab is {tab}px wide around a 13px mark and a {label}px label, so it is not sized by \
         what it holds"
    );
}

/// The same, in a container that wraps.
///
/// `flex-wrap: wrap` changes how a line is filled, not how an item is measured — an item still
/// wants to be as wide as its content, and a wrapping row of them still has to ask.
#[test]
fn a_wrapping_flex_row_still_sizes_its_items_by_their_content() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("tab").classes(&["tab"]).children(vec![
                Element::new("mark").classes(&["mark"]),
                Element::new("item").classes(&["label"]).text("Elements"),
            ]),
        ]),
        "root { display: flex; flex-direction: row; flex-wrap: wrap; width: 400px;
                align-items: flex-start }
         tab { display: flex; flex-direction: row; flex-grow: 0; flex-shrink: 0 }
         mark { display: block; width: 13px; height: 13px }
         item { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 200.0);

    let label = width_of(&store, 3);
    let tab = width_of(&store, 1);
    assert!(
        label > 0.0,
        "in a wrapping row the label laid out {label}px wide"
    );
    assert!(
        tab >= label + 13.0,
        "in a wrapping row the tab is {tab}px around a 13px mark and a {label}px label"
    );
}

/// An *inline* element used as a flex item is blockified, so it takes room like any other.
///
/// CSS says a flex item's `display` is blockified: `inline` computes to `block`. An engine that
/// skips that leaves the item with no box of its own, and a label written as inline text — which is
/// what `text` is — lays out at nothing inside every flex row in the program.
#[test]
fn an_inline_flex_item_is_blockified_and_takes_room() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("mark").classes(&["mark"]),
            Element::new("item").classes(&["label"]).text("Elements"),
        ]),
        "root { display: flex; flex-direction: row; width: 400px; align-items: flex-start }
         mark { display: block; width: 13px; height: 13px }
         item { display: inline }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 200.0);

    let label = width_of(&store, 2);
    assert!(
        label > 0.0,
        "an inline flex item holding `Elements` laid out {label}px wide, so it was never blockified"
    );
}
