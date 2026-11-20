//! The paint order mirrored as the compositor's order tree.

mod support;

use support::compositor::Recorded;
use support::{Element, Harness};
use zgui_interned::ClassName;

/// Three overlapping positioned squares in red, green and blue, painted in that order, beside a
/// list of ten rows.
fn stack() -> Harness {
    let rows = (0..10).map(|_| Element::new("row")).collect();
    Harness::sized(
        Element::new("root").children(vec![
            Element::new("stack").children(vec![
                Element::new("r"),
                Element::new("g"),
                Element::new("b"),
            ]),
            Element::new("list").children(rows),
        ]),
        "root { display: block; width: 200px; height: 200px }
         stack { display: block; height: 60px }
         r, g, b { display: block; position: relative; width: 40px; height: 40px; margin-top: -20px }
         r { background: red } g { background: lime } b { background: blue }
         .over { z-index: 1 }
         row { display: block; height: 10px; background: gray }",
        200.0,
        200.0,
    )
}

/// The fills of the quads the last frame drew, in the order it drew them.
fn fills(output: &Recorded) -> Vec<&'static str> {
    let drawing = output.output.drawing().unwrap_or_default();
    drawing
        .lines()
        .filter_map(|line| {
            [
                ("premul[1, 0, 0, 1]", "red"),
                ("premul[0, 1, 0, 1]", "green"),
                ("premul[0, 0, 1, 1]", "blue"),
            ]
            .into_iter()
            .find(|(fill, _)| line.contains(fill))
            .map(|(_, name)| name)
        })
        .collect()
}

#[test]
fn a_z_index_change_rebuilds_only_the_list_of_the_parent() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = stack();
    output.frame_everything(&mut harness);
    let before = fills(&output);

    let red = harness.element("r");
    harness.edit_and_restyle(|edit| edit.add_class(red, ClassName::new("over")));
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [red]);
    harness.compose_from_marks(200.0, 200.0);
    let written = output.frame_everything(&mut harness);

    if !before.is_empty() {
        assert_eq!(before, ["red", "green", "blue"]);
        assert_eq!(fills(&output), ["green", "blue", "red"]);
    }
    // The stack's three entries out and in again, and the raised square's own chunk in the node
    // it now gets as a stacking context. Nothing of the list beside them.
    assert_eq!(written.report.order_entries, 7, "{written:?}");
    assert_eq!(written.commit.keys_spliced, 7, "{written:?}");
}

#[test]
fn a_box_that_stops_being_a_stacking_context_goes_back_into_its_parent_list() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = stack();
    output.frame_everything(&mut harness);
    let red = harness.element("r");
    for (class, add) in [("over", true), ("over", false)] {
        harness.edit_and_restyle(|edit| {
            if add {
                edit.add_class(red, ClassName::new(class));
            } else {
                edit.remove_class(red, ClassName::new(class));
            }
        });
        zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [red]);
        harness.compose_from_marks(200.0, 200.0);
        output.frame_everything(&mut harness);
    }

    let fills = fills(&output);
    if !fills.is_empty() {
        assert_eq!(fills, ["red", "green", "blue"]);
    }
}
