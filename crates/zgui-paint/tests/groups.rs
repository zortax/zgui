//! Groups drawn through the effects of compositor order nodes.

mod support;

use support::compositor::Recorded;
use support::{Element, Harness};
use zgui_interned::ClassName;

/// A card over two overlapping squares, styled with `card` and the classes the test swaps in.
fn card(card: &str) -> Harness {
    Harness::sized(
        Element::new("root").children(vec![
            Element::new("card").children(vec![Element::new("a"), Element::new("b")]),
        ]),
        &format!(
            "root {{ display: block; width: 200px; height: 200px; background: white }}
             card {{ display: block; width: 100px; height: 100px; {card} }}
             a, b {{ display: block; width: 40px; height: 40px; background: blue }}
             b {{ margin-top: -20px }}
             .half {{ opacity: 0.5 }}
             .faint {{ opacity: 0.25 }}
             .gone {{ opacity: 0 }}"
        ),
        200.0,
        200.0,
    )
}

/// Gives the element `name` the classes `classes` and composes the change.
fn restyle(harness: &mut Harness, name: &str, classes: &[&str]) {
    let element = harness.element(name);
    let classes: Vec<ClassName> = classes.iter().map(|class| ClassName::new(class)).collect();
    harness.edit_and_restyle(|edit| edit.set_classes(element, &classes));
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [element]);
    harness.compose_from_marks(200.0, 200.0);
}

/// Draws the first frame of `harness`, and returns the transcript of what it drew.
fn drawing(harness: &mut Harness) -> Option<(Recorded, String)> {
    let mut output = Recorded::new([200, 200])?;
    output.frame_everything(harness);
    let drawing = output.output.drawing()?;
    Some((output, drawing))
}

#[test]
fn an_opacity_over_overlapping_children_draws_one_group() {
    let mut harness = card("opacity: 0.5");
    let Some((output, drawing)) = drawing(&mut harness) else {
        return;
    };

    assert_eq!(output.output.counters().frame.groups, 1, "{drawing}");
    assert_holds(&drawing, &["group", "opacity=0.5"]);
}

#[test]
fn a_filter_draws_a_group_with_its_chain() {
    let mut harness = card("filter: blur(3px) grayscale(1)");
    let Some((output, drawing)) = drawing(&mut harness) else {
        return;
    };

    assert_eq!(output.output.counters().frame.groups, 1, "{drawing}");
    assert_holds(&drawing, &["blur", "matrix"]);
}

#[test]
fn a_blend_mode_draws_a_group_that_blends() {
    let mut harness = card("mix-blend-mode: multiply");
    let Some((output, drawing)) = drawing(&mut harness) else {
        return;
    };

    assert!(output.output.counters().frame.groups >= 1, "{drawing}");
    assert_holds(&drawing, &["multiply"]);
}

#[test]
fn an_opacity_over_a_box_alone_folds_into_its_paint() {
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("a")]),
        "root { display: block; width: 200px; height: 200px }
         a { display: block; width: 40px; height: 40px; background: blue; opacity: 0.5 }",
        200.0,
        200.0,
    );
    let Some((output, drawing)) = drawing(&mut harness) else {
        return;
    };

    assert_eq!(output.output.counters().frame.groups, 0, "{drawing}");
    // The colour is carried in the reference at eight bits, so 0.5 is 128 / 255.
    assert_holds(&drawing, &["premul[0, 0, 0.502, 0.502]"]);
}

#[test]
fn a_group_faded_out_draws_nothing_of_its_subtree() {
    let mut harness = card("");
    let Some((mut output, before)) = drawing(&mut harness) else {
        return;
    };
    assert!(before.contains("premul[0, 0, 1, 1]"), "{before}");

    restyle(&mut harness, "card", &["gone"]);
    output.frame_everything(&mut harness);
    let after = output.output.drawing().unwrap_or_default();

    assert!(!after.contains("premul[0, 0, 1, 1]"), "{after}");
}

#[test]
fn a_new_opacity_on_a_group_writes_its_effect_and_changes_no_chunk() {
    let mut harness = card("");
    let Some((mut output, _)) = drawing(&mut harness) else {
        return;
    };
    restyle(&mut harness, "card", &["half"]);
    output.frame(&mut harness);

    restyle(&mut harness, "card", &["faint"]);
    let written = output.frame(&mut harness);

    // The card's own empty chunk is recorded again for its new style, and stays identical.
    assert_eq!(
        written.commit.chunks_identical, written.commit.chunks_recorded,
        "{written:?}"
    );
    assert_eq!(written.commit.property_writes, 1, "{written:?}");
}

/// Asserts that `drawing` holds every one of `parts`.
fn assert_holds(drawing: &str, parts: &[&str]) {
    for part in parts {
        assert!(drawing.contains(part), "no `{part}` in:\n{drawing}");
    }
}

#[test]
fn a_group_cut_by_a_rounded_frame_holds_the_frame_clip_until_both_go() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("frame").children(vec![
            Element::new("card")
                .classes(&["group"])
                .children(vec![Element::new("a"), Element::new("b")]),
        ])]),
        "root { display: block; width: 200px; height: 200px; background: white }
         frame { display: block; width: 120px; height: 120px; overflow: hidden;
                 border-radius: 12px }
         card { display: block; width: 100px; height: 100px }
         .group { filter: blur(2px) }
         a, b { display: block; width: 40px; height: 40px; background: blue }
         b { margin-top: -20px }",
        200.0,
        200.0,
    );
    output.frame_everything(&mut harness);
    assert_eq!(output.output.counters().frame.groups, 1);

    restyle(&mut harness, "card", &[]);
    let ungrouped = output.frame(&mut harness);
    let frame = harness.element("frame");
    harness.edit_and_restyle(|edit| edit.remove(frame));
    harness.rebuild(200.0, 200.0);
    let removed = output.frame(&mut harness);

    assert_eq!(ungrouped.report.released, 0, "{ungrouped:?}");
    assert!(removed.report.removed > 0, "{removed:?}");
    // The blue is carried in the quads' paint references, so it held no slot.
    assert_eq!(removed.report.released, 1, "the frame's clip: {removed:?}");
}
