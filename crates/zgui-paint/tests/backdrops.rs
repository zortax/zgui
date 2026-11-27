//! Backdrop filters, recorded as chunks of their own outside the group of their box.

mod support;

use support::{Element, Harness};

/// A card that frosts what is under it and fades as a group, over a striped ground.
fn card(css: &str) -> Harness {
    Harness::new(
        Element::new("root").children(vec![
            Element::new("ground"),
            Element::new("card").children(vec![Element::new("label")]),
        ]),
        &format!(
            "root {{ display: block; width: 300px; height: 200px }}
             ground {{ display: block; height: 100px; background: rgb(200, 0, 0) }}
             card {{ display: block; height: 80px; margin-top: -60px; background: white;
                    backdrop-filter: blur(8px) }}
             label {{ display: block; height: 20px; background: black }}
             card.gone {{ opacity: 0 }}
             {css}"
        ),
    )
}

/// The position of the first line of the transcript of `drawing` that starts with `prefix`.
fn line_of(drawing: &zgui_testkit_compositor::Drawing, prefix: &str) -> usize {
    drawing
        .transcript()
        .to_string()
        .lines()
        .position(|line| line.trim_start().starts_with(prefix))
        .unwrap_or_else(|| panic!("no `{prefix}` in {drawing}"))
}

#[test]
fn a_backdrop_draws_over_what_is_under_its_box_and_under_its_group() {
    let mut harness = card("card { opacity: 0.5 }");
    harness.paint_everything();
    let drawing = harness.redraw();
    let ground = drawing
        .quads()
        .iter()
        .position(|quad| quad.color("fill") == Some([0.7843, 0.0, 0.0, 1.0]))
        .expect("the ground");
    assert_eq!(ground, 0, "the ground draws first: {drawing}");
    let backdrop = line_of(&drawing, "backdrop ");
    let group = line_of(&drawing, "group_begin ");
    assert!(
        backdrop < group,
        "the backdrop reads what is under the card, outside the card's group: {drawing}"
    );
}

#[test]
fn a_box_faded_to_nothing_leaves_no_backdrop() {
    let mut harness = card("");
    harness.paint_everything();
    assert_eq!(harness.redraw().objects("backdrop").count(), 1);

    let card = harness.element("card");
    harness.edit_and_restyle(|edit| edit.add_class(card, zgui_interned::ClassName::new("gone")));
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [card]);
    harness.compose_from_marks(400.0, 400.0);
    harness.paint_everything();
    let drawing = harness.redraw();
    assert_eq!(drawing.objects("backdrop").count(), 0, "{drawing}");
}
