//! What a floated box does to the lines beside it.

mod support;

use support::{Element, Fixture, lay_out, measurer};
use zgui_layout::BoxKey;
use zgui_layout::inline::lines::LineBox;
use zgui_layout::tree::store::LayoutStore;

/// The first box that establishes an inline formatting context.
fn inline_root(store: &LayoutStore) -> BoxKey {
    let mut stack = vec![store.root().expect("a root")];
    while let Some(key) = stack.pop() {
        if store.inline_resolution(key).is_some() {
            return key;
        }
        stack.extend(store.node(key).children.iter().copied());
    }
    panic!("no inline formatting context was laid out");
}

/// Lays out a paragraph beside a float of the given height and returns its lines.
fn beside_float(height: f32) -> Vec<LineBox> {
    let css = format!(
        "root {{ display: block; width: 200px }}
         side {{ display: block; float: left; width: 80px; height: {height}px }}
         para {{ display: block }}"
    );
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("side"),
            Element::new("para").text("alpha bravo delta gamma kappa sigma omega alpha bravo"),
        ]),
        &css,
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 200.0, 600.0);
    store
        .inline_resolution(inline_root(&store))
        .expect("laid out")
        .lines
        .clone()
}

#[test]
fn the_lines_level_with_a_float_are_narrower_and_the_ones_below_it_are_not() {
    // The float is 80 wide and 48 tall, which is exactly two lines of this paragraph. So the first
    // two lines start 80 in and have 120 to fill, and every line after them has the whole 200.
    let lines = beside_float(48.0);
    assert!(lines.len() > 3, "the paragraph has to reach past the float");

    for line in lines.iter().take(2) {
        assert_eq!(line.offset, 80.0, "a line beside the float starts after it");
        assert!(
            line.width <= 120.0,
            "a line beside the float is {} wide, which is the whole box",
            line.width
        );
    }
    for line in lines.iter().skip(2) {
        assert_eq!(
            line.offset, 0.0,
            "a line below the float starts at the edge"
        );
    }
    assert!(
        lines.iter().skip(2).any(|line| line.width > 120.0),
        "no line below the float used the width the float gave back"
    );
}

#[test]
fn a_float_that_covers_the_whole_paragraph_narrows_every_line() {
    // The control for the test above: with a float as tall as the paragraph, *every* line is
    // narrow, so "the ones below it are not" is a statement about the float's height rather than
    // about the first two lines being special.
    let lines = beside_float(400.0);
    assert!(lines.len() > 3);
    for line in &lines {
        assert_eq!(line.offset, 80.0);
        assert!(line.width <= 120.0);
    }
}

#[test]
fn a_paragraph_with_no_float_beside_it_takes_the_whole_width() {
    // And the control for both: the same paragraph with nothing floated is not banded at all.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("para").text("alpha bravo delta gamma kappa sigma omega alpha bravo"),
        ]),
        "root { display: block; width: 200px }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 200.0, 600.0);
    let lines = store
        .inline_resolution(inline_root(&store))
        .expect("laid out")
        .lines
        .clone();
    assert!(lines.iter().all(|line| line.offset == 0.0));
    assert!(lines.iter().any(|line| line.width > 120.0));
}
