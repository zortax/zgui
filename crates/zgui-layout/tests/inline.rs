//! The line boxes an inline formatting context resolves to.
//!
//! Every number below is arithmetic a reader can do: the deterministic shaper behind these fixtures
//! advances eight device pixels a character at the initial font size, and reports a face 12.8 above
//! the baseline and 3.2 below it. The cascade's own `line-height` then adds four pixels of
//! half-leading on each side, so an ordinary line is 24 tall with its baseline 16.8 down.

mod support;

use support::{Element, Fixture, lay_out, measurer, measurer_with_images};
use zgui_layout::BoxKey;
use zgui_layout::inline::lines::LineBox;
use zgui_layout::inline::resolved::InlineResolution;
use zgui_layout::tree::store::LayoutStore;

/// One character's advance at the initial font size.
const ADVANCE: f32 = 8.0;
/// The face's own ascent at that size.
const ASCENT: f32 = 12.8;
/// Its descent.
const DESCENT: f32 = 3.2;
/// Half the leading the cascaded `line-height` adds either side of the content area.
const HALF_LEADING: f32 = 4.0;
/// The distance from a line box's top to its baseline, for a line of ordinary text.
const BASELINE: f32 = ASCENT + HALF_LEADING;
/// An ordinary line's height.
const LINE: f32 = ASCENT + DESCENT + 2.0 * HALF_LEADING;

/// Asserts two lengths agree to within a sixtieth of a device pixel, which is the tolerance a
/// position is indistinguishable at.
#[track_caller]
fn close(left: f32, right: f32, what: &str) {
    assert!(
        (left - right).abs() <= 1.0 / 60.0,
        "{what}: {left} is not {right}"
    );
}

/// The first box that establishes an inline formatting context, in tree order.
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

/// Lays out one fixture and returns the store plus what its first context resolved to.
fn resolve(tree: Element, css: &str, width: f32) -> (LayoutStore, InlineResolution) {
    let fixture = Fixture::new(tree, css);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, width, 600.0);
    let key = inline_root(&store);
    let resolution = store
        .inline_resolution(key)
        .expect("the context was laid out")
        .clone();
    (store, resolution)
}

#[test]
fn a_paragraph_breaks_into_lines_that_are_stacked_by_their_own_extents() {
    // Six words of five characters each: forty pixels a word, plus a space between them.
    let (_, resolution) = resolve(
        Element::new("root").children(vec![
            Element::new("para").text("alpha bravo delta gamma kappa sigma"),
        ]),
        "root { display: block; width: 200px }
         para { display: block }",
        200.0,
    );
    assert!(
        resolution.lines.len() > 1,
        "two hundred pixels does not hold thirty-five characters"
    );
    for (index, line) in resolution.lines.iter().enumerate() {
        assert_eq!(line.height(), LINE, "line {index}");
        assert_eq!(line.top, index as f32 * LINE, "line {index}");
        assert_eq!(line.baseline(), line.top + BASELINE, "line {index}");
        assert!(line.width <= 200.0, "line {index} overflows its width");
    }
}

#[test]
fn line_box_matches_analytic() {
    // The line box is recomputed here from the inputs the shaper reports — the face's own ascent
    // and descent, the leading the cascade asks for, and the image's margin box — rather than from
    // anything the context computed on the way. CSS 2.1 §10.8: the line box reaches as far above
    // the baseline as the tallest thing on it and as far below as the deepest, each measured after
    // its own alignment shift, and the strut is one of the things on it.
    let (store, resolution) = resolve_with_image(
        "root { display: block; width: 400px }
         para { display: block }
         picture { display: inline; vertical-align: baseline }",
        400.0,
        (60.0, 40.0),
    );

    let line = &resolution.lines[0];
    // The image sits with its bottom margin edge on the baseline, so it reaches its whole height
    // above it and nothing below.
    let above = f32::max(ASCENT + HALF_LEADING, 40.0);
    let below = f32::max(DESCENT + HALF_LEADING, 0.0);
    assert_eq!(line.extents.above, above);
    assert_eq!(line.extents.below, below);
    assert_eq!(line.height(), above + below);
    assert_eq!(line.baseline(), above);

    // And the image's own top edge is that far above the baseline.
    let placement = resolution.placements.first().expect("the image was placed");
    assert_eq!(placement.origin.1, line.baseline() - 40.0);
    let image = store.layout_of(placement.box_).expect("laid out");
    assert_eq!(image.size.height.0, 40.0);
}

/// Lays out a paragraph holding one image between two words.
fn resolve_with_image(
    css: &str,
    width: f32,
    natural: (f32, f32),
) -> (LayoutStore, InlineResolution) {
    let fixture = Fixture::with_natural_size(
        Element::new("root").children(vec![Element::new("para").children(vec![
            Element::new("lead").text("one "),
            Element::new("picture").image(natural.0, natural.1),
            Element::new("tail").text(" two"),
        ])]),
        css,
        natural,
    );
    let mut store = fixture.box_tree();
    let mut content = measurer_with_images(natural.0, natural.1);
    lay_out(&mut store, &mut content, width, 600.0);
    let key = inline_root(&store);
    let resolution = store
        .inline_resolution(key)
        .expect("the context was laid out")
        .clone();
    (store, resolution)
}

/// Where the image's top edge sits under one `vertical-align` value, and how tall the line is.
fn aligned_image(value: &str) -> (LineBox, f32) {
    let css = format!(
        "root {{ display: block; width: 400px }}
         para {{ display: block }}
         picture {{ display: inline; vertical-align: {value} }}"
    );
    let (_, resolution) = resolve_with_image(&css, 400.0, (60.0, 40.0));
    let line = resolution.lines[0].clone();
    let top = resolution
        .placements
        .first()
        .expect("the image was placed")
        .origin
        .1;
    (line, top)
}

#[test]
fn every_vertical_align_keyword_moves_the_box_the_distance_it_names() {
    // Seven keywords and a percentage, each against the line box and the strut it is measured from.
    // The image is 40 tall with its own baseline at its bottom edge.
    let (baseline, top) = aligned_image("baseline");
    close(top, baseline.baseline() - 40.0, "baseline");

    // A superscript is raised by a third of the parent's font size, and the line grows to hold it.
    let (line, top) = aligned_image("super");
    let raised = 16.0 * zgui_layout::inline::vertical_align::SUPER_FRACTION;
    close(line.extents.above, 40.0 + raised, "super raises the line");
    close(top, line.baseline() - 40.0 - raised, "super");

    // A subscript is lowered by a fifth of it, which pushes the line box down rather than up.
    let (line, top) = aligned_image("sub");
    let lowered = 16.0 * zgui_layout::inline::vertical_align::SUB_FRACTION;
    close(top, line.baseline() - 40.0 + lowered, "sub");
    close(
        line.extents.below,
        lowered.max(DESCENT + HALF_LEADING),
        "sub deepens the line",
    );

    // `text-top` puts the box's top edge on the top of the parent's *content area*, which is the
    // face's own ascent above the baseline and not the line box's top.
    let (line, top) = aligned_image("text-top");
    close(top, line.baseline() - ASCENT, "text-top");

    // `text-bottom` puts its bottom edge on the bottom of that area.
    let (line, top) = aligned_image("text-bottom");
    close(top + 40.0, line.baseline() + DESCENT, "text-bottom");

    // `middle` puts the box's midpoint on the baseline raised by half the parent's x-height.
    let (line, top) = aligned_image("middle");
    close(top + 20.0, line.baseline() - 8.0 / 2.0, "middle");

    // A length raises it by exactly that length.
    let (line, top) = aligned_image("10px");
    close(top, line.baseline() - 40.0 - 10.0, "a length");

    // A percentage is of the aligned box's own line height, which is 24 here.
    let (line, top) = aligned_image("50%");
    close(top, line.baseline() - 40.0 - 12.0, "a percentage");
}

#[test]
fn the_line_relative_keywords_take_a_second_breaking_pass() {
    // `top` and `bottom` align with the line box's own edges, which are not known until everything
    // else on the line has been placed. The shift therefore cannot be baked into the height the
    // shaper is told before breaking, and the context has to break again once it knows.
    //
    // The line is made 120 tall by its own `line-height`, so it reaches 64.8 above its baseline
    // against the image's 40. That is what makes the test discriminating: on a line whose *above*
    // extent is the image's own height, a `top`-aligned image and a baseline-aligned one land in
    // exactly the same place, and the assertion passes with the second pass never running. Line
    // height, not line box height — a line 47.2 tall whose content sits 40 above the baseline is
    // still a line the image is the tallest thing on.
    let css = "root { display: block; width: 400px }
         para { display: block; line-height: 120px }
         picture { display: inline; vertical-align: top }";
    let (_, resolution) = resolve_with_image(css, 400.0, (60.0, 40.0));
    let line = &resolution.lines[0];
    let baseline_placed = {
        let control = css.replace("vertical-align: top", "vertical-align: baseline");
        let (_, control) = resolve_with_image(&control, 400.0, (60.0, 40.0));
        control.placements[0].origin.1
    };
    assert!(
        line.extents.above > 40.0,
        "the image is the tallest thing above the baseline, so `top` and `baseline` put it in the \
         same place and the case cannot tell them apart"
    );
    let top = resolution.placements[0].origin.1;
    assert_eq!(top, line.top, "a top-aligned box starts at the line's top");
    assert!(
        (top - baseline_placed).abs() > 1.0,
        "the same box aligned to the baseline lands at {baseline_placed} too, so nothing was \
         resolved against the line box"
    );

    let css = css.replace("vertical-align: top", "vertical-align: bottom");
    let (_, resolution) = resolve_with_image(&css, 400.0, (60.0, 40.0));
    let line = &resolution.lines[0];
    let top = resolution.placements[0].origin.1;
    assert!(
        line.extents.below > 0.0,
        "nothing reaches below the baseline, so `bottom` and `baseline` agree"
    );
    assert_eq!(
        top + 40.0,
        line.top + line.height(),
        "a bottom-aligned box ends at the line's bottom"
    );
    assert!(
        (top - baseline_placed).abs() > 1.0,
        "a bottom-aligned box landed where a baseline-aligned one does"
    );
}

#[test]
fn a_nested_inline_box_takes_up_its_own_margins_and_padding() {
    // An inline box has no size of its own, and its horizontal margin, border and padding are still
    // on the line: they push the text after them along, and they count against the width the line
    // has to fit into.
    let plain = resolve(
        Element::new("root").children(vec![
            Element::new("para").children(vec![Element::new("span").text("abcd")]),
        ]),
        "root { display: block; width: 400px }
         para { display: block }
         span { display: inline }",
        400.0,
    );
    let padded = resolve(
        Element::new("root").children(vec![
            Element::new("para").children(vec![Element::new("span").text("abcd")]),
        ]),
        "root { display: block; width: 400px }
         para { display: block }
         span { display: inline; padding-left: 7px; margin-right: 5px; border-left: 3px solid black }",
        400.0,
    );
    assert_eq!(plain.1.lines[0].width, 4.0 * ADVANCE);
    assert_eq!(
        padded.1.lines[0].width,
        4.0 * ADVANCE + 7.0 + 5.0 + 3.0,
        "the span's own edges are on the line"
    );
}

#[test]
fn text_indent_moves_the_first_line_and_nothing_else() {
    let (_, resolution) = resolve(
        Element::new("root").children(vec![
            Element::new("para").text("alpha bravo delta gamma kappa sigma"),
        ]),
        "root { display: block; width: 200px }
         para { display: block; text-indent: 24px }",
        200.0,
    );
    assert!(resolution.lines.len() > 1);
    assert_eq!(resolution.lines[0].offset, 24.0);
    assert_eq!(resolution.lines[1].offset, 0.0);
}

#[test]
fn tab_size_decides_how_far_a_preserved_tab_advances() {
    // A tab is not a character with an advance of its own: `tab-size` says how far it moves the
    // text after it. Under the default of eight it is eight spaces wide, and under `tab-size: 2`
    // it is two.
    let wide = resolve(
        Element::new("root").children(vec![Element::new("para").text("a\tb")]),
        "root { display: block; width: 400px }
         para { display: block; white-space: pre }",
        400.0,
    );
    let narrow = resolve(
        Element::new("root").children(vec![Element::new("para").text("a\tb")]),
        "root { display: block; width: 400px }
         para { display: block; white-space: pre; tab-size: 2 }",
        400.0,
    );
    assert_eq!(wide.1.lines[0].width, 10.0 * ADVANCE, "a, eight spaces, b");
    assert_eq!(narrow.1.lines[0].width, 4.0 * ADVANCE, "a, two spaces, b");
}

#[test]
fn a_taller_run_makes_only_its_own_line_taller() {
    // Every run on a line contributes its own face and its own half-leading, and the tallest wins
    // on each side independently. A line box taken from one run's metrics would be wrong for every
    // paragraph that mixes sizes.
    let (_, resolution) = resolve(
        Element::new("root").children(vec![Element::new("para").children(vec![
            Element::new("small").text("aaaa bbbb cccc"),
            Element::new("big").text(" X"),
        ])]),
        "root { display: block; width: 120px }
         para { display: block }
         big { display: inline; font-size: 32px }",
        120.0,
    );
    assert!(resolution.lines.len() > 1, "the paragraph has to wrap");
    let last = resolution.lines.last().expect("a line");
    assert!(
        last.height() > LINE,
        "the larger run did not reach its own line",
    );
    assert_eq!(
        resolution.lines[0].height(),
        LINE,
        "and it did not reach the lines above it",
    );
}

#[test]
fn the_context_reports_the_first_and_the_last_line_as_two_different_baselines() {
    let (store, resolution) = resolve(
        Element::new("root").children(vec![
            Element::new("para").text("alpha bravo delta gamma kappa sigma"),
        ]),
        "root { display: block; width: 200px }
         para { display: block }",
        200.0,
    );
    assert!(resolution.lines.len() > 1);
    let first = resolution.first_baseline().expect("a first baseline");
    let last = resolution.last_baseline().expect("a last baseline");
    assert!(last > first, "a wrapped paragraph has two of them");

    let key = inline_root(&store);
    let layout = store.layout_of(key).expect("laid out");
    assert_eq!(layout.first_baseline.expect("reported").0, first);
    assert_eq!(layout.last_baseline.expect("reported").0, last);
}

#[test]
fn every_line_becomes_one_fragment() {
    let (store, resolution) = resolve(
        Element::new("root").children(vec![
            Element::new("para").text("alpha bravo delta gamma kappa sigma"),
        ]),
        "root { display: block; width: 200px }
         para { display: block }",
        200.0,
    );
    let key = inline_root(&store);
    let fragments = store.fragments_of_box(key);
    // The box's own piece — its background, its border, its decorations — comes first, and one
    // piece per line follows it.
    assert_eq!(fragments.len(), resolution.lines.len() + 1);
    assert!(matches!(
        store.fragment(fragments[0]).expect("live").kind,
        zgui_layout::FragmentKind::Box
    ));
    let content_top = store
        .fragment(fragments[0])
        .expect("live")
        .content_box
        .origin
        .y
        .0;
    for (index, (fragment, line)) in fragments[1..].iter().zip(&resolution.lines).enumerate() {
        let fragment = store.fragment(*fragment).expect("live");
        assert_eq!(
            fragment.border_box.origin.y.0,
            content_top + line.top,
            "fragment {index} sits where its line does"
        );
        assert_eq!(fragment.border_box.size.height.0, line.height());
        assert!(
            matches!(
                fragment.kind,
                zgui_layout::FragmentKind::Line { line: at, .. } if usize::from(at) == index
            ),
            "fragment {index} is not that line",
        );
    }
}

#[test]
fn laying_out_again_replaces_the_lines_rather_than_adding_to_them() {
    // A context is laid out again whenever anything about it changes, and the lines it had before
    // are not the lines it has now. Fragments that accumulated would be painted twice over.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("para").text("alpha bravo delta gamma kappa sigma"),
        ]),
        "root { display: block; width: 200px }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 200.0, 600.0);
    let key = inline_root(&store);
    let after_one = store.fragments_of_box(key).len();

    zgui_layout::tree::dirty::mark_dirty(&mut store, key);
    lay_out(&mut store, &mut content, 200.0, 600.0);
    assert_eq!(store.fragments_of_box(key).len(), after_one);
}

#[test]
fn vertical_align_on_a_run_of_text_moves_nothing_yet() {
    // The scheme this context resolves `vertical-align` with is the shaper's one lever: the height
    // an *inline box* is declared at. A run of text is not an inline box — it is a styled range of
    // the one string the shaper was handed — and there is no per-run baseline offset anywhere
    // between here and the glyphs. So `<sup>` and `<sub>`, which is what almost every author means
    // by `vertical-align`, do nothing at all: the run keeps its place on the baseline and the line
    // box keeps the height the strut gave it.
    //
    // This is written down as an assertion rather than left absent because the failure is silent —
    // an author writes the property and no error is reported anywhere. Closing it needs a shift
    // carried on `StyledRun` through to wherever glyphs are positioned, which is a text-engine and
    // a painting change, not a layout one; when it lands, this case fails and says so.
    let (_, plain) = resolve(
        Element::new("root").children(vec![
            Element::new("para").children(vec![Element::new("run").text("x2")]),
        ]),
        "root { display: block; width: 400px }
         para { display: block }
         run { display: inline }",
        400.0,
    );
    let (_, raised) = resolve(
        Element::new("root").children(vec![
            Element::new("para").children(vec![Element::new("run").text("x2")]),
        ]),
        "root { display: block; width: 400px }
         para { display: block }
         run { display: inline; vertical-align: super }",
        400.0,
    );
    assert_eq!(
        plain.lines[0].extents, raised.lines[0].extents,
        "a superscripted run now changes the line box, so the run-level shift has a producer and \
         this case has to become an assertion about where the glyphs went"
    );
    assert_eq!(plain.lines[0].height(), LINE);

    // The control that keeps the case honest: the same property on something that *is* an inline
    // box does move it, so the assertion above is about runs and not about the property being
    // unread.
    let (line, top) = aligned_image("super");
    close(
        top,
        line.baseline() - 40.0 - 16.0 * zgui_layout::inline::vertical_align::SUPER_FRACTION,
        "an atomic inline is raised",
    );
}
