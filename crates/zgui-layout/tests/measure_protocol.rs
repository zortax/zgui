//! What the layout algorithms actually ask a paragraph, and how often.
//!
//! The criterion is not "one question per measure call". A layout algorithm asks a leaf many times
//! and most of those questions are free: an intrinsic probe is answered from the shaped glyphs and
//! costs no line breaking at all, and the size-then-layout pair at one width costs one break
//! between them. So the two numbers worth asserting are the number of *shapes*, which must be one
//! per paragraph, and the number of *breaks*, which must be one per distinct width.

mod support;

use support::{Element, Fixture, lay_out, measurer};

/// What one layout cost the shaper.
struct Cost {
    /// How many paragraphs were shaped.
    shapes: u32,
    /// How many breaking passes ran.
    breaks: u32,
    /// The widths those passes were asked for, in order.
    widths: Vec<Option<f32>>,
}

/// Lays a fixture out and reports what the shaper did.
fn cost(fixture: &Fixture, viewport: (f32, f32)) -> (zgui_layout::LayoutStore, Cost) {
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, viewport.0, viewport.1);
    let shaper = content.shaper();
    let cost = Cost {
        shapes: shaper.shapes,
        breaks: shaper.breaks,
        widths: shaper.widths.clone(),
    };
    (store, cost)
}

#[test]
fn block_layout_asks_exactly_once() {
    // Block layout stretch-fits its child and measures it once, with no intrinsic probing at all.
    // Two-pass probing is a flex and grid property; a cost model built on the block case is wrong
    // by several times for a component library, where most containers are flex.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("item").text("hello there")]),
        "root { display: block; width: 400px }
         item { display: block }",
    );
    let (_, cost) = cost(&fixture, (400.0, 300.0));

    assert_eq!(cost.shapes, 1, "one paragraph, one shape");
    assert_eq!(
        cost.breaks, 1,
        "block layout broke the paragraph {} times at {:?}",
        cost.breaks, cost.widths
    );
    assert!(
        cost.widths.iter().all(Option::is_some),
        "block layout took an intrinsic break, at {:?}",
        cost.widths
    );
}

#[test]
fn a_flex_row_shapes_once_per_leaf_and_breaks_once_per_distinct_width() {
    // The control for the test above, and the phase's own criterion. A flex row probes each leaf at
    // minimum and maximum content and then lays it out, which is several calls per leaf and few
    // breaks: the probes are answered from the shaped glyphs, and the size-then-layout pair at one
    // width is one break between them.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("item").text("one two"),
            Element::new("item").text("three four"),
        ]),
        "root { display: flex; width: 400px }
         item { display: block }",
    );
    let (_, cost) = cost(&fixture, (400.0, 300.0));

    assert_eq!(cost.shapes, 2, "two leaves, two shapes");
    assert_eq!(
        cost.breaks as usize,
        cost.widths.len(),
        "a breaking pass ran without being recorded"
    );
    let distinct: std::collections::BTreeSet<u32> = cost
        .widths
        .iter()
        .map(|width| width.unwrap_or(f32::INFINITY).to_bits())
        .collect();
    assert_eq!(
        cost.breaks as usize,
        distinct.len(),
        "{} breaking passes over {} distinct widths: {:?}",
        cost.breaks,
        distinct.len(),
        cost.widths
    );
    assert!(
        cost.breaks < 8,
        "a two-leaf flex row cost {} breaking passes, so the probes are breaking",
        cost.breaks
    );
}

#[test]
fn breaks_at_the_width_the_engine_assigns() {
    // The width the measure step is handed has to equal the box's own final content-box width, or
    // lines are broken at one width and painted at another. The classic failure is padding and
    // border being taken off twice, or not at all.
    for (padding, border) in [(0.0_f32, 0.0_f32), (17.0, 3.0)] {
        let css = format!(
            "root {{ display: block; width: 400px }}
             item {{ display: block; padding: {padding}px; border: {border}px solid black }}"
        );
        let fixture = Fixture::new(
            Element::new("root").children(vec![Element::new("item").text("hello there")]),
            &css,
        );
        let (store, cost) = cost(&fixture, (400.0, 300.0));

        let asked = cost
            .widths
            .last()
            .copied()
            .flatten()
            .expect("the paragraph was broken at a definite width");

        // The leaf here is the anonymous box wrapping the run of text, one level below the item.
        let root = store.root().expect("a root");
        let item = store.node(root).children[0];
        let leaf = store.node(item).children[0];
        let content_width = store
            .layout_of(leaf)
            .expect("laid out")
            .content_box()
            .size
            .width
            .0;

        assert!(
            (asked - content_width).abs() <= 1.0 / 60.0,
            "broken at {asked} but the content box is {content_width} \
             (padding {padding}, border {border})"
        );
    }
}

#[test]
fn the_final_pass_is_told_that_it_is_the_one_being_kept() {
    // A probe may be answered from anything already computed; the kept answer is the one whose
    // side effects have to be real. A measurer that could not tell them apart would either persist
    // a probe's answer or persist none at all.
    let fixture = Fixture::with_natural_size(
        Element::new("root").children(vec![Element::new("item").image(60.0, 40.0)]),
        "root { display: flex; width: 400px }
         item { display: block }",
        (60.0, 40.0),
    );
    let mut store = fixture.box_tree();
    let mut content = support::measurer_with_images(60.0, 40.0);
    lay_out(&mut store, &mut content, 400.0, 300.0);

    let asks = &content.replaced_mut().asks;
    assert!(!asks.is_empty(), "the replaced box was never measured");
    assert!(
        asks.iter().any(|ask| ask.final_pass),
        "no ask was marked as the one being kept"
    );
    assert!(
        asks.iter().any(|ask| !ask.final_pass),
        "every ask was marked as being kept, so the flag says nothing"
    );
}

#[test]
fn the_atomic_memo_answers_a_repeated_constraint_without_a_second_nested_layout() {
    // An atomic inline costs a whole nested layout per measurement, and the algorithms measure it
    // at the same constraint several times over. Without the memo those repeats are real layouts.
    //
    // The container is a flex one on purpose: block layout measures its child once, so a block
    // fixture never asks the same question twice and the memo is never consulted for an answer it
    // holds. A `misses > 0` assertion over that fixture passes while the memo does nothing.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("cell").children(vec![Element::new("item").text("one")]),
        ]),
        "root { display: flex; width: 400px }
         cell { display: block }
         item { display: inline-block; width: 60px; height: 30px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut tree = zgui_layout::tree::LayoutTree::new(
        &mut store,
        &mut content,
        zgui_layout::style::DeviceStyle::default(),
    );
    assert!(tree.layout_root(taffy::Size {
        width: 400.0,
        height: 300.0
    }));
    let memo = tree.atomic_memo();
    assert!(
        memo.misses() > 0,
        "no atomic inline was measured at all, so the memo is untested"
    );
    assert!(
        memo.hits() > 0,
        "the atomic inline was measured {} times and the memo answered none of them, \
         so every repeat cost a whole nested layout",
        memo.misses()
    );
}
