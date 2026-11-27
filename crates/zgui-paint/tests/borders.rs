//! The colour a box's border is stroked with, one per side where the sides differ.

mod support;

use support::{Element, Harness};

/// The stroke paint the compositor draws the one bordered quad `css` gives `mark` with.
fn stroke(css: &str) -> String {
    let sheet = format!(
        "root {{ display: block; width: 200px; height: 100px }}
         mark {{ display: block; width: 100px; height: 40px; {css} }}"
    );
    let mut harness = Harness::new(
        Element::new("root").children(vec![Element::new("mark")]),
        Box::leak(sheet.into_boxed_str()),
    );
    harness.paint_everything();
    let drawing = harness.redraw();
    let quad = drawing
        .quads()
        .into_iter()
        .find(|quad| quad.field("border").is_some())
        .unwrap_or_else(|| panic!("a bordered quad: {drawing}"));
    quad.paint("stroke").expect("a bordered quad has a stroke")
}

#[test]
fn a_side_of_another_colour_strokes_each_side_in_its_own() {
    let blue = "premul[0, 0, 1, 1]";
    let red = "premul[1, 0, 0, 1]";
    assert_eq!(
        stroke("border: 1px solid blue; border-left: 4px solid red"),
        format!("sides [{blue}, {blue}, {blue}, {red}]")
    );
}

#[test]
fn a_transparent_border_with_one_coloured_side_keeps_the_others_transparent() {
    let clear = "premul[0, 0, 0, 0]";
    assert_eq!(
        stroke("border: 1px solid transparent; border-left-color: red"),
        format!("sides [{clear}, {clear}, {clear}, premul[1, 0, 0, 1]]"),
        "one visible side among transparent ones is a sides stroke"
    );
}

#[test]
fn one_colour_on_every_side_with_a_width_is_a_solid_stroke() {
    // Only the left side has a width, so the others' colours do not count.
    assert_eq!(
        stroke("color: blue; border-left: 4px solid red"),
        "solid premul[1, 0, 0, 1]"
    );
    assert_eq!(stroke("border: 2px solid red"), "solid premul[1, 0, 0, 1]");
}
