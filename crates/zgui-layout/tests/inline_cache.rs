//! What a re-layout costs a paragraph that has already been shaped.
//!
//! The whole arrangement exists for one number: shaping is expensive and breaking is cheap, and a
//! layout algorithm asks a paragraph how big it is at many widths. Every test here is about which
//! of the two a question cost.

mod support;

use support::text::{first_inline_root, inline_roots, lines, paragraph};
use support::{Element, Fixture, lay_out, measurer};
use zgui_layout::{BoxKind, BoxNode, FormattingContext};

#[test]
fn width_changes_never_reshape() {
    // Forty widths over one paragraph. The shaped glyphs do not depend on the width, so not one of
    // them may cost a shape — and every distinct width must cost a break, or the answer is stale
    // rather than cheap.
    let text: &'static str = Box::leak(paragraph(200).into_boxed_str());
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").text(text)]),
        "root { display: block }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();

    lay_out(&mut store, &mut content, 400.0, 4000.0);
    assert_eq!(content.shaper().shapes, 1, "the first layout shapes once");
    let after_first = content.shaper().breaks;

    for step in 0..40 {
        let width = 200.0 + step as f32 * 10.0;
        let key = *inline_roots(&store).first().expect("a context");
        zgui_layout::tree::dirty::mark_dirty(&mut store, key);
        lay_out(&mut store, &mut content, width, 4000.0);
    }

    assert_eq!(
        content.shaper().shapes,
        1,
        "forty width changes re-shaped the paragraph {} times",
        content.shaper().shapes
    );
    assert!(
        content.shaper().breaks > after_first + 30,
        "forty width changes cost only {} breaking passes, so the widths are not reaching the \
         breaker at all",
        content.shaper().breaks - after_first
    );
}

#[test]
fn a_rebreak_equals_a_fresh_shape_at_the_same_width() {
    // The counter above proves nothing was re-shaped. Only this proves the answer is still right:
    // a paragraph broken down through four widths on a warm cache has to agree, line for line,
    // with one shaped at that width from nothing.
    let text: &'static str = Box::leak(paragraph(120).into_boxed_str());
    let css = "root { display: block }
         para { display: block }";
    let element = || Element::new("root").children(vec![Element::new("para").text(text)]);

    let fixture = Fixture::new(element(), css);
    let mut warm_store = fixture.box_tree();
    let mut warm = measurer();
    lay_out(&mut warm_store, &mut warm, 800.0, 4000.0);

    for width in [600.0, 420.0, 300.0, 220.0] {
        let key = *inline_roots(&warm_store).first().expect("a context");
        zgui_layout::tree::dirty::mark_dirty(&mut warm_store, key);
        lay_out(&mut warm_store, &mut warm, width, 4000.0);

        let fresh_fixture = Fixture::new(element(), css);
        let mut fresh_store = fresh_fixture.box_tree();
        let mut fresh = measurer();
        lay_out(&mut fresh_store, &mut fresh, width, 4000.0);

        assert_eq!(fresh.shaper().shapes, 1, "the fresh tree shaped once");
        assert_eq!(
            lines(&warm_store),
            lines(&fresh_store),
            "re-broken at {width} the warm paragraph disagrees with a fresh one"
        );
    }
    assert_eq!(warm.shaper().shapes, 1, "the warm tree never re-shaped");
}

#[test]
fn changing_vertical_align_moves_the_box_without_reshaping() {
    // The alignment shift is folded into the height the shaper was told, so nothing in the shaped
    // glyphs can notice it changed. Against a warm cache a re-style is therefore a silent no-op
    // unless the shift is resolved afresh on every measure call — the box does not move at all, and
    // no error is reported anywhere.
    //
    // The obvious version of this test resizes instead of re-styling, and passes while the
    // mechanism does nothing.
    let natural = (60.0, 40.0);
    // The line is taller than the image, so where the image sits inside it is a real position
    // rather than the line's own top edge: a test whose box is the tallest thing on the line
    // measures nothing, because raising it only makes the line taller.
    let mut fixture = Fixture::with_natural_size(
        Element::new("root").children(vec![Element::new("para").children(vec![
            Element::new("lead").text("one "),
            Element::new("picture").image(natural.0, natural.1),
        ])]),
        "root { display: block; width: 400px }
         para { display: block; line-height: 120px }
         picture { display: inline; vertical-align: baseline }
         .raised { vertical-align: super }",
        natural,
    );

    let mut content = support::measurer_with_images(natural.0, natural.1);

    let mut store = fixture.box_tree();
    lay_out(&mut store, &mut content, 400.0, 600.0);
    let key = *inline_roots(&store).first().expect("a context");
    let before = store.inline_resolution(key).expect("laid out").placements[0].origin;
    let shapes_before = content.shaper().shapes;

    // The same document, re-styled through the mutation protocol: a real re-style keeps every
    // unchanged style group's allocation, and the caches key on those pointers — a fresh fixture
    // shares them only by allocator accident. The glyphs are identical, so the cache is warm.
    let paragraph = fixture
        .document
        .store()
        .core(fixture.root)
        .first_child()
        .expect("the paragraph");
    let lead = fixture
        .document
        .store()
        .core(paragraph)
        .first_child()
        .expect("the lead text");
    let picture = fixture
        .document
        .store()
        .core(lead)
        .next_sibling()
        .expect("the picture");
    fixture.edit_and_restyle(|edit| {
        edit.add_class(picture, zgui_interned::ClassName::new("raised"));
    });
    let mut raised_store = fixture.box_tree();
    lay_out(&mut raised_store, &mut content, 400.0, 600.0);
    let key = *inline_roots(&raised_store).first().expect("a context");
    let after = raised_store
        .inline_resolution(key)
        .expect("laid out")
        .placements[0]
        .origin;

    assert_eq!(
        content.shaper().shapes,
        shapes_before,
        "the re-style re-shaped the paragraph"
    );
    let raised = 16.0 * zgui_layout::inline::vertical_align::SUPER_FRACTION;
    assert!(
        (before.1 - after.1 - raised).abs() <= 1.0 / 60.0,
        "the box moved by {} where the superscript offset is {raised}",
        before.1 - after.1
    );
}

#[test]
fn a_keystroke_in_a_200_line_textarea_reshapes_one_paragraph() {
    // Two hundred paragraphs, one of them edited. Shaping is per paragraph, so the cost of the
    // edit is one shape — the failure this guards is a smooth quadratic that no golden can see.
    // Each paragraph says something different, because a shaped result is held against its content
    // and two hundred identical paragraphs would share one entry — one shape for all of them, and
    // an edit to any of them costing one shape whatever the mechanism did.
    let mut children = Vec::new();
    for index in 0..200 {
        let text: &'static str =
            Box::leak(format!("alpha bravo delta gamma {index}").into_boxed_str());
        children.push(Element::new("para").text(text));
    }
    let mut fixture = Fixture::new(
        Element::new("root").children(children),
        "root { display: block; width: 400px }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 20000.0);
    assert_eq!(
        content.shaper().shapes,
        200,
        "each paragraph shaped exactly once"
    );

    // One character typed into the first paragraph.
    let root = fixture.document.root_index().expect("a root");
    let first = fixture
        .document
        .store()
        .core(root)
        .first_child()
        .expect("a paragraph");
    let text_node = fixture
        .document
        .store()
        .core(first)
        .first_child()
        .expect("a text node");
    fixture.edit_and_restyle(|edit| {
        edit.set_text(text_node, "alpha bravo delta gamma 0 typed");
    });

    let mut edited = fixture.box_tree();
    lay_out(&mut edited, &mut content, 400.0, 20000.0);
    assert_eq!(
        content.shaper().shapes,
        201,
        "a keystroke re-shaped {} paragraphs",
        content.shaper().shapes - 200
    );
}

#[test]
fn twenty_widths_cost_twenty_breaking_passes_and_one_flattening() {
    // The work a run of widths is allowed to cost, stated in the two quantities that decide it: a
    // width has to reach the breaker, and none of them may re-do the width-independent half. What
    // that costs in wall-clock time is asserted by the `budgets` target, which the gate runs in an
    // optimised build — an unoptimised time is not a measurement of anything.
    //
    // The two guards that stop this passing on a paragraph that never wrapped: the context has to
    // hold at least two hundred lines, and every width has to cost a real breaking pass.
    let text: &'static str = Box::leak(paragraph(2400).into_boxed_str());
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").text(text)]),
        "root { display: block }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 300.0, 40000.0);
    assert!(
        lines(&store).len() >= 200,
        "the fixture broke into {} lines, so this is measuring nothing",
        lines(&store).len()
    );

    let before = content.shaper().breaks;
    for step in 0..20 {
        let width = 300.0 + step as f32 * 3.0;
        let key = first_inline_root(&store);
        zgui_layout::tree::dirty::mark_dirty(&mut store, key);
        lay_out(&mut store, &mut content, width, 40000.0);
    }
    let passes = content.shaper().breaks - before;

    assert!(
        passes >= 19,
        "twenty widths cost only {passes} breaking passes"
    );
    assert_eq!(content.shaper().shapes, 1, "and not one re-shape");
    assert!(
        lines(&store).len() >= 200,
        "the last width has to wrap as much as the first"
    );
    assert_eq!(
        store.flattenings(),
        1,
        "twenty widths flattened the paragraph into the shaper's string {} times",
        store.flattenings()
    );
}

#[test]
fn a_run_of_text_replaced_inside_a_context_flattens_it_again() {
    // The other half of the case above. A flattened paragraph is held against the boxes it was
    // built from, and a restyle or an edit patches the tree by putting a fresh box in place of an
    // old one under a parent that keeps its identity — so the context this holds is the one whose
    // held form has to be dropped. A memo that only ever grew stale in the other direction would
    // satisfy every count above and would leave the old text on the screen for ever.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").children(vec![
            Element::new("lead").text("alpha bravo "),
            Element::new("tail").text("delta gamma"),
        ])]),
        "root { display: block; width: 400px }
         para { display: block }
         lead { display: inline }
         tail { display: inline }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 4000.0);
    assert_eq!(store.flattenings(), 1, "the first layout flattened once");
    let whole = lines(&store)[0].width;

    // The last span's run of text, replaced by a shorter one. Nothing above it moves: the context
    // and the span both keep the keys they had.
    let context = first_inline_root(&store);
    let tail = *store
        .node(context)
        .children
        .last()
        .expect("the context holds both spans");
    let run = store.node(tail).children[0];
    let style = store.node(run).style.clone();
    let shorter = store
        .insert(BoxNode::new(style, BoxKind::TextRun, FormattingContext::Inline).with_text("x"));
    assert!(zgui_layout::boxtree::patch::replace(
        &mut store, run, shorter
    ));
    store.recycle();
    lay_out(&mut store, &mut content, 400.0, 4000.0);

    assert_eq!(
        store.flattenings(),
        2,
        "the context was handed a different run of text and was not flattened again"
    );
    assert!(
        lines(&store)[0].width < whole,
        "the line is {} wide holding `alpha bravo x` and was {whole} wide holding `alpha bravo \
         delta gamma`",
        lines(&store)[0].width
    );
}

#[test]
fn moving_a_window_to_a_denser_display_flattens_every_paragraph_again() {
    // The other thing a flattened paragraph is held against. Everything the layout algorithms work
    // in is device pixels, so the paragraph style inside a flattened context — its text indent —
    // was scaled by the ratio of the display it was built for. A window dragged onto a display with
    // a different ratio invalidates every box's layout, and a held form that survived that would
    // indent the first line of every paragraph by the old display's number for ever.
    let text: &'static str = Box::leak(paragraph(40).into_boxed_str());
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").text(text)]),
        "root { display: block; width: 300px }
         para { display: block; text-indent: 20px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    support::lay_out_at_scale(&mut store, &mut content, 300.0, 4000.0, 1.0);
    assert_eq!(store.flattenings(), 1, "the first layout flattened once");
    let at_one = lines(&store)[0].offset;

    zgui_layout::tree::dirty::mark_all_dirty(&mut store);
    support::lay_out_at_scale(&mut store, &mut content, 600.0, 8000.0, 2.0);

    assert_eq!(
        store.flattenings(),
        2,
        "the display's density changed and the paragraph was not flattened again"
    );
    assert!(
        (lines(&store)[0].offset - at_one * 2.0).abs() < 0.5,
        "the first line is indented by {} at twice the density and was indented by {at_one} at one",
        lines(&store)[0].offset
    );
}

#[test]
fn re_colouring_a_document_touches_no_shaped_paragraph() {
    // A brush is a slot in a table, never a colour, so changing what a run is drawn in is a write
    // into that table. If a colour were part of what a paragraph is cached under, switching theme
    // would re-shape every string in the application.
    let text: &'static str = Box::leak(paragraph(40).into_boxed_str());
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").text(text)]),
        "root { display: block; width: 300px }
         para { display: block; color: rgb(10, 20, 30) }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 300.0, 4000.0);
    let held = content.cache().len();
    assert_eq!(held, 1, "one paragraph is held");

    let mut recoloured = 0;
    content.paints_mut().recolour(|_, mut paint| {
        recoloured += 1;
        paint.color = [1.0, 1.0, 1.0, 1.0];
        paint
    });
    assert!(recoloured > 0, "no run claimed a brush slot at all");
    assert_eq!(content.cache().len(), held, "the shaped paragraph survived");
    assert_eq!(content.shaper().shapes, 1, "and was not shaped again");
}
