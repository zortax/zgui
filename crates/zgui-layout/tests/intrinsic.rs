//! What a box sized by a content keyword comes out at.

mod support;

use support::{Element, Fixture, lay_out, measurer};
use zgui_layout::BoxKey;
use zgui_layout::tree::store::LayoutStore;

/// The width of the box `store`'s root holds, and of the box that one holds.
fn widths(store: &LayoutStore) -> Vec<f32> {
    let mut out = Vec::new();
    let mut key: Option<BoxKey> = store.root();
    while let Some(current) = key {
        out.push(store.layout_of(current).expect("laid out").size.width.0);
        key = store.node(current).children.first().copied();
    }
    out
}

/// Lays out one fixture. Text measures eight device pixels a character, so `hello there` is 88
/// pixels at its widest and 40 at its narrowest — the longest word it cannot break inside.
fn lay_out_fixture(tree: Element, css: &str) -> LayoutStore {
    let fixture = Fixture::new(tree, css);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 300.0);
    store
}

#[test]
fn a_keyword_inside_a_keyword_is_answered_from_the_inside_out() {
    // The inner box is as narrow as its content can be, so the outer box — which is as wide as its
    // content wants — is as wide as *that*, not as wide as the text would have been unconstrained.
    // Measuring the outer box first reads the inner one's width as `auto` and comes out three times
    // too wide, with nothing in the result to say that a keyword was ignored.
    let store = lay_out_fixture(
        Element::new("root").children(vec![
            Element::new("outer").children(vec![Element::new("inner").text("hello there")]),
        ]),
        "root { display: block; width: 400px }
         outer { display: block; width: max-content }
         inner { display: block; width: min-content }",
    );
    let widths = widths(&store);
    assert_eq!(widths[0], 400.0, "the root fills the viewport");
    assert_eq!(widths[1], 40.0, "the outer box is as wide as the inner one");
    assert_eq!(widths[2], 40.0, "the inner box is as narrow as its content");
}

#[test]
fn a_content_keyword_is_the_content_box_and_not_the_border_box() {
    // A measurement is of the whole box, insets included. Under `box-sizing: content-box` the
    // algorithms add the padding back on to whatever the style asked for, so handing them the
    // measurement unchanged pays for the padding twice — here, 60 device pixels of it.
    let store = lay_out_fixture(
        Element::new("root").children(vec![Element::new("item").text("hello there")]),
        "root { display: block; width: 400px }
         item { display: block; width: max-content; padding: 30px; box-sizing: content-box }",
    );
    let root = store.root().expect("a root");
    let item = store.node(root).children[0];
    let layout = store.layout_of(item).expect("laid out");
    assert_eq!(layout.size.width.0, 148.0, "border box");
    assert_eq!(layout.content_box().size.width.0, 88.0, "content box");
}

#[test]
fn the_two_box_sizing_modes_agree_on_a_content_keyword() {
    // The control: `max-content` names the content's own width in both modes, so the box it
    // produces is the same box. A fix applied to one mode and not the other would show here.
    let border_box = lay_out_fixture(
        Element::new("root").children(vec![Element::new("item").text("hello there")]),
        "root { display: block; width: 400px }
         item { display: block; width: max-content; padding: 30px; box-sizing: border-box }",
    );
    let content_box = lay_out_fixture(
        Element::new("root").children(vec![Element::new("item").text("hello there")]),
        "root { display: block; width: 400px }
         item { display: block; width: max-content; padding: 30px; box-sizing: content-box }",
    );
    assert_eq!(widths(&border_box), widths(&content_box));
}

#[test]
fn a_box_sized_by_an_atomic_inline_is_as_wide_as_that_box_and_not_as_its_contents() {
    // The shape of a control beside a heading: a block holding one atomic inline, whose own line
    // holds one atomic inline of another width. The two contexts flatten identically — no text, one
    // box at offset zero — and an identifier is a position in one flattened form, so nothing in
    // them but the widths tells them apart. Sharing one shaped result gives the outer block the
    // inner mark's width, and every sibling on its row is then laid out around a box 16 too narrow.
    let store = lay_out_fixture(
        Element::new("root").children(vec![Element::new("wrap").children(vec![
            Element::new("control").children(vec![
                Element::new("mark").children(vec![Element::new("art")]),
            ]),
        ])]),
        "root { display: flex; flex-direction: row; align-items: center; width: 400px }
         wrap { display: block; flex: 0 0 auto }
         control { display: inline-flex; align-items: center; width: 30px; height: 30px }
         mark { display: block }
         art { display: inline-block; width: 14px; height: 14px }",
    );
    let widths = widths(&store);
    assert_eq!(widths[1], 30.0, "the block is as wide as the control in it");
}

#[test]
fn a_max_content_box_holding_an_inline_block_asks_it_how_wide_it_wants_to_be() {
    // The narrow answer and the wide one are two different questions about the same atomic inline,
    // and the box around it is sized by the second. A shaped result that held the figures the first
    // one produced would size this box to the longest word.
    let store = lay_out_fixture(
        Element::new("root").children(vec![
            Element::new("wrap").children(vec![Element::new("inner").text("hello there")]),
        ]),
        "root { display: block; width: 400px }
         wrap { display: block; width: max-content }
         inner { display: inline-block }",
    );
    let widths = widths(&store);
    assert_eq!(widths[1], 88.0, "the whole sentence, not its longest word");
}
