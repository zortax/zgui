//! Where a box's baseline comes from, in the two cases that differ.

mod support;

use support::{Element, Fixture, lay_out, measurer};
use zgui_layout::tree::store::LayoutStore;

/// The stylesheet both tests share, differing only in the container's `display`.
fn css(display: &str) -> String {
    format!(
        "root {{ display: {display}; align-items: baseline; width: 400px }}
         item {{ display: block; width: 100px }}"
    )
}

/// A container with two block-level children, each holding one text run.
fn fixture(display: &str) -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![
            Element::new("item").text("one"),
            Element::new("item").text("two"),
        ]),
        &css(display),
    )
}

/// The first baseline of every box, in tree order.
fn baselines(store: &LayoutStore) -> Vec<Option<f32>> {
    let mut out = Vec::new();
    let mut stack = vec![store.root().expect("a root")];
    while let Some(key) = stack.pop() {
        out.push(
            store
                .layout_of(key)
                .and_then(|layout| layout.first_baseline)
                .map(|baseline| baseline.0),
        );
        stack.extend(store.node(key).children.iter().copied());
    }
    out
}

#[test]
fn flex_containers_propagate_a_baseline_unaided() {
    // A flex container computes its own first baseline, from the first baseline-aligned item on
    // its first line. Filling one in on top of that would replace a real answer with one derived
    // from the first child, which is a different box the moment any item is baseline-aligned.
    let fixture = fixture("flex");
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 300.0);

    let root = store.root().expect("a root");
    let baseline = store
        .layout_of(root)
        .expect("laid out")
        .first_baseline
        .expect("a flex container reports its own first baseline");
    // The container's baseline is an item's baseline, offset by where that item sits — not the
    // container's own height and not zero.
    assert!(baseline.0 > 0.0);
    let item = store.node(root).children[0];
    let item_layout = store.layout_of(item).expect("laid out");
    assert!(baseline.0 <= item_layout.size.height.0 + item_layout.origin.y.0);
}

#[test]
fn block_containers_need_our_fill_in() {
    // A block container reports no baseline at all, so one is filled in from its first in-flow
    // child. Without it, a baseline-aligned row of block-level items aligns on the bottom margin
    // edge instead, which is a different number by more than a pixel.
    //
    // This is a separate test from the flex one on purpose. A single test that switched the
    // fill-in off would be circular: the fill-in overwrites, so switching it off destroys the flex
    // container's real baseline rather than revealing an absent one, and the flex case then looks
    // as though it needed what it does not.
    let fixture = fixture("block");
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 300.0);

    let root = store.root().expect("a root");
    let baseline = store
        .layout_of(root)
        .expect("laid out")
        .first_baseline
        .expect("a block container's baseline is filled in from its first in-flow child");

    // It is the first child's baseline moved down by where that child sits, and the first child is
    // at the top, so the two agree exactly.
    let first = store.node(root).children[0];
    let first_layout = store.layout_of(first).expect("laid out");
    let child_baseline = first_layout
        .first_baseline
        .expect("the child reports one of its own");
    assert_eq!(baseline.0, child_baseline.0 + first_layout.origin.y.0);

    // And it is not the container's height, which is what an unaligned row would have used.
    let height = store.layout_of(root).expect("laid out").size.height.0;
    assert_ne!(baseline.0, height);
}

#[test]
fn every_box_that_reports_a_baseline_reports_a_last_one_too() {
    // A multi-row leaf's last baseline is below its first, because CSS aligns an inline-block in
    // normal flow on its last line box. A single baseline for both would put a two-row box a whole
    // row too high.
    let fixture = fixture("block");
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 100.0, 300.0);

    assert!(
        baselines(&store).iter().any(Option::is_some),
        "no box reported a baseline at all"
    );
    let mut saw_a_multi_row_leaf = false;
    let mut stack = vec![store.root().expect("a root")];
    while let Some(key) = stack.pop() {
        let layout = store.layout_of(key).expect("laid out");
        if let (Some(first), Some(last)) = (layout.first_baseline, layout.last_baseline)
            && last.0 > first.0
        {
            saw_a_multi_row_leaf = true;
        }
        stack.extend(store.node(key).children.iter().copied());
    }
    assert!(
        saw_a_multi_row_leaf,
        "the fixture has to wrap, or the last baseline is untested"
    );
}

/// Lays out one `inline-block` between two words and reports the line it landed on, where its top
/// edge went, and how tall it came out.
fn inline_block(extra: &str, text: &'static str) -> (f32, f32, f32) {
    let css = format!(
        "root {{ display: block; width: 400px }}
         para {{ display: block }}
         thing {{ display: inline-block; width: 40px; {extra} }}"
    );
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").children(vec![
            Element::new("lead").text("x "),
            Element::new("thing").text(text),
        ])]),
        &css,
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 600.0);
    let key = *inline_roots(&store).first().expect("a context");
    let resolution = store.inline_resolution(key).expect("laid out");
    let line = &resolution.lines[0];
    let placement = resolution.placements[0];
    let height = store
        .layout_of(placement.box_)
        .expect("laid out")
        .size
        .height
        .0;
    (line.baseline(), placement.origin.1, height)
}

/// Every box that establishes an inline formatting context.
fn inline_roots(store: &LayoutStore) -> Vec<zgui_layout::BoxKey> {
    let mut out = Vec::new();
    let mut stack = vec![store.root().expect("a root")];
    while let Some(key) = stack.pop() {
        if store.inline_resolution(key).is_some() {
            out.push(key);
        }
        stack.extend(store.node(key).children.iter().copied());
    }
    out
}

#[test]
fn an_inline_block_sits_on_the_line_by_its_last_line_box() {
    // The mono shaper advances eight pixels a character, so a 40 px `inline-block` holding
    // "aaaaa bbbbb" wraps into two 24 px lines and its own last baseline is 24 + 16.8 down.
    // CSS 2.1 §10.8.1 puts *that* on the line's baseline, so the box hangs 7.2 below it.
    let (baseline, top, height) = inline_block("", "aaaaa bbbbb");
    assert_eq!(height, 48.0, "the box has to wrap, or it has one baseline");
    assert_eq!(
        top,
        baseline - 40.8,
        "the box was aligned by something other than its last line box"
    );
    assert!(
        top + height > baseline,
        "a two-line inline-block hangs below the baseline it sits on; this one sits entirely \
         above it, which is what aligning on the bottom margin edge looks like"
    );

    // The control: one line of text, and the box's only baseline is 16.8 down.
    let (baseline, top, height) = inline_block("", "aaa");
    assert_eq!(height, 24.0);
    assert_eq!(top, baseline - 16.8);
}

#[test]
fn an_inline_block_that_clips_its_content_sits_on_its_bottom_margin_edge() {
    // The exception in the same paragraph of the specification, and the reason the rule above is
    // not simply "use the last line": a box whose content can be scrolled out of sight has no
    // baseline anything outside it can align to, so its bottom margin edge is the baseline.
    for overflow in ["overflow: hidden", "overflow: clip", "overflow: scroll"] {
        let (baseline, top, height) = inline_block(overflow, "aaaaa bbbbb");
        assert!(
            height >= 48.0,
            "{overflow}: the box has to wrap, or it has one line and the two rules agree"
        );
        assert_eq!(
            top + height,
            baseline,
            "{overflow} still aligned the box by a line box inside it"
        );
        // The control that stops the assertion above passing on a box that happens to be as tall
        // as its own last baseline: a `visible` box of the same content is aligned higher.
        let (visible_baseline, visible_top, _) = inline_block("", "aaaaa bbbbb");
        assert!(
            visible_top + 48.0 > visible_baseline,
            "the visible control has to hang below its baseline"
        );
    }
}

#[test]
fn a_block_container_takes_its_baseline_from_its_first_in_flow_child() {
    // A float is laid out beside the flow rather than in it, so it is not a box the container
    // takes a baseline from — CSS 2.1 §10.8.1 says the *first in-flow* line box. The float here is
    // in a much larger face, so a container that took its baseline would be out by 25 pixels and
    // every baseline-aligned row holding it would sit wrong.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("col").children(vec![
            Element::new("side").text("f"),
            Element::new("para").text("body"),
        ])]),
        "root { display: block; width: 400px }
         col { display: block }
         side { display: block; float: left; width: 40px; font-size: 40px }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 600.0);

    let root = store.root().expect("a root");
    let col = store.node(root).children[0];
    let float = store.node(col).children[0];
    let para = store.node(col).children[1];
    let baseline_of = |key| {
        store
            .layout_of(key)
            .expect("laid out")
            .first_baseline
            .expect("a baseline")
            .0
    };
    assert_ne!(
        baseline_of(float),
        baseline_of(para),
        "the two candidates have to differ, or the case cannot tell which was taken"
    );
    assert_eq!(
        baseline_of(col),
        baseline_of(para),
        "the container took the floated child's baseline"
    );
}
