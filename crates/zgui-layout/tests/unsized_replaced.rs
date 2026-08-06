//! A replaced box whose content has no intrinsic size yet.
//!
//! This is the state every image is in between mount and decode, and every embedded surface is in
//! between mount and its first frame: the node carries the replaced flag, the installed source
//! answers [`Intrinsic::default`](zgui_dom::host::Intrinsic), and layout has to produce a sensible
//! box from that — CSS-sized when the author said so, collapsed when they did not, and never a
//! panic. Nothing exercised this path before the runtime started installing a real source, which
//! is exactly why it is pinned here.

mod support;

use support::{Element, Fixture, lay_out, measurer};
use zgui_layout::FormattingContext;

/// Lays the fixture out and returns the border-box size of `root`'s first child box.
fn first_child_size(fixture: &Fixture, viewport: (f32, f32)) -> (f32, f32) {
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, viewport.0, viewport.1);
    let root = store.root().expect("a root box");
    let child = *store
        .node(root)
        .children
        .first()
        .expect("the replaced element built a box");
    assert_eq!(
        store.node(child).fc,
        FormattingContext::Replaced,
        "the flag alone makes the box replaced; no intrinsic answer is needed"
    );
    let fragment = *store
        .fragments_of_box(child)
        .first()
        .expect("the replaced box composed a fragment");
    let fragment = store.fragment(fragment).expect("live");
    (
        fragment.border_box.size.width.0,
        fragment.border_box.size.height.0,
    )
}

/// The fixture: one replaced element under a block root, replaced by *nothing yet*.
fn fixture(css: &str) -> Fixture {
    Fixture::with_unknown_intrinsics(
        Element::new("root").children(vec![Element::new("picture").image(0.0, 0.0)]),
        css,
    )
}

#[test]
fn an_unsized_replaced_box_collapses_instead_of_panicking() {
    // No author size, no intrinsic: the box is there, is replaced, and the pass finishes. The
    // inline axis stretches because that is what block layout does to any block-level box with
    // `width: auto`; the block axis has nothing to say — no content, no ratio — and collapses.
    // Zero *area* is the contract: nothing invented an extent the content will later contradict.
    let fixture = fixture("root { display: block; width: 400px } picture { display: block }");
    assert_eq!(first_child_size(&fixture, (400.0, 300.0)), (400.0, 0.0));
}

#[test]
fn css_sizes_an_unsized_replaced_box_alone() {
    // The common case on the first frame: the author gave the element a size, the decode has not
    // landed, and the box must already be the size the content will fill — that is what keeps the
    // decode's arrival from moving the page.
    let fixture = fixture(
        "root { display: block; width: 400px }
         picture { display: block; width: 100px; height: 50px }",
    );
    assert_eq!(first_child_size(&fixture, (400.0, 300.0)), (100.0, 50.0));
}

#[test]
fn one_definite_axis_does_not_conjure_the_other() {
    // With a width and no height, a real image would supply the ratio that decides the height.
    // Before it exists there is no ratio, and the height the author left auto resolves to zero
    // rather than to a guess.
    let fixture = fixture(
        "root { display: block; width: 400px }
         picture { display: block; width: 100px }",
    );
    assert_eq!(first_child_size(&fixture, (400.0, 300.0)), (100.0, 0.0));
}
