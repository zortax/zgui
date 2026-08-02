//! The declaration that makes a box's background paint the text inside it.
//!
//! `background-clip` is a property this engine build discards, so `background-clip: text` cannot be
//! read from a cascade result. The custom-property scheme is the same one drawings are themed
//! through, and this is what says it survives the cascade rather than being a value the paint stage
//! could only be handed directly.

mod support;

use support::{Element, Harness};

/// The lowered paint style of a named element.
fn style_of(harness: &Harness, name: &str) -> zgui_paint::lower::PaintStyle {
    let key = harness.box_of(name);
    zgui_paint::lower::lower(&harness.store().node(key).style, 1.0)
}

/// A root with one child.
fn tree() -> Element {
    Element::new("root").children(vec![Element::new("mark")])
}

#[test]
fn the_declaration_moves_the_ramp_from_the_box_onto_its_text() {
    let css = "root { display: block; width: 200px; height: 100px }
               mark { display: block; width: 120px; height: 40px;
                      background-image: linear-gradient(90deg, rgb(255, 0, 0), rgb(0, 0, 255));
                      --zgui-text-fill: background }";
    let harness = Harness::new(tree(), css);
    let style = style_of(&harness, "mark");

    let ramp = style.text_fill.expect("the ramp paints the text");
    assert_eq!(ramp.stops.len(), 2, "the ramp is the one that was written");
    assert!(
        style.background.layers.is_empty(),
        "a ramp painting the text is a ramp the box no longer paints"
    );
}

#[test]
fn a_box_that_does_not_ask_paints_its_own_background() {
    let css = "root { display: block; width: 200px; height: 100px }
               mark { display: block; width: 120px; height: 40px;
                      background-image: linear-gradient(90deg, rgb(255, 0, 0), rgb(0, 0, 255)) }";
    let harness = Harness::new(tree(), css);
    let style = style_of(&harness, "mark");

    assert!(style.text_fill.is_none(), "nothing asked for it");
    assert_eq!(
        style.background.layers.len(),
        1,
        "and the box keeps its own"
    );
}

/// The property inherits, which is what themes a whole heading and its inline spans at once — and
/// a box with no ramp of its own to hand over paints its text in `color` as it always did.
#[test]
fn asking_without_a_ramp_changes_nothing() {
    let css = "root { display: block; width: 200px; height: 100px;
                      --zgui-text-fill: background }
               mark { display: block; width: 120px; height: 40px }";
    let harness = Harness::new(tree(), css);
    let style = style_of(&harness, "mark");

    assert!(
        style.text_fill.is_none(),
        "there is no ramp to paint the text with, so the text is painted in `color`"
    );
}
