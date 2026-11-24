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

/// The fills of the quads a frame of the whole output draws, in the order it draws them.
fn fills(output: &mut Recorded) -> Vec<&'static str> {
    output.output.redraw();
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
    let before = fills(&mut output);

    let red = harness.element("r");
    harness.edit_and_restyle(|edit| edit.add_class(red, ClassName::new("over")));
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [red]);
    harness.compose_from_marks(200.0, 200.0);
    let written = output.frame_everything(&mut harness);

    if !before.is_empty() {
        assert_eq!(before, ["red", "green", "blue"]);
        assert_eq!(fills(&mut output), ["green", "blue", "red"]);
    }
    // The raised square's chunk out of the stack's list, its new node in at the end, and its chunk
    // in that node. Nothing of the green and blue squares, and nothing of the list beside them.
    assert_eq!(written.report.order_entries, 3, "{written:?}");
    assert_eq!(written.commit.keys_spliced, 3, "{written:?}");
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

    let fills = fills(&mut output);
    if !fills.is_empty() {
        assert_eq!(fills, ["red", "green", "blue"]);
    }
}

#[test]
fn removed_rows_leave_their_list_without_emptying_their_nodes() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let row = || Element::new("row").children(vec![Element::new("cell"), Element::new("cell")]);
    let mut harness = Harness::sized(
        Element::new("root").children(vec![
            Element::new("list").children(
                (0..10)
                    .map(|_| row())
                    .chain([Element::new("tail")])
                    .collect(),
            ),
        ]),
        "root { display: block; width: 200px; height: 200px }
         list { display: block }
         row { display: block; height: 10px; background: gray }
         cell { display: block; height: 5px; background: blue }
         tail { display: block; height: 10px }",
        200.0,
        200.0,
    );
    output.frame_everything(&mut harness);
    let root = harness.document.root_index().expect("a root element");
    // The marks of the first build, which built the whole tree.
    zgui_layout::boxtree::retire(&mut harness.document, root);

    let rows = harness.elements("row");
    harness.edit_and_restyle(|edit| {
        for row in rows {
            edit.remove(row);
        }
    });
    let owed = zgui_layout::boxtree::retire(&mut harness.document, root);
    zgui_layout::boxtree::patch::rebuild(&mut harness.store, &harness.document, &owed)
        .expect("the rows leave the box tree");
    harness.compose_from_marks(200.0, 200.0);
    let written = output.frame(&mut harness);

    // The ten row nodes out of the list, and nothing out of the nodes, which are removed whole.
    assert_eq!(written.report.order_entries, 10, "{written:?}");
    assert_eq!(written.commit.keys_spliced, 10, "{written:?}");
    assert!(fills(&mut output).is_empty());
}
