//! How long mirrored nodes and slots live: while a recorded chunk or a mirrored node names them.

mod support;

use support::compositor::Recorded;
use support::{Element, Harness};
use zgui_interned::ClassName;

/// Two boxes painted red, the second in a clipping, transformed frame.
fn boxes() -> Harness {
    Harness::sized(
        Element::new("root").children(vec![
            Element::new("a").classes(&["red"]),
            Element::new("frame").children(vec![Element::new("b").classes(&["red"])]),
        ]),
        "root { display: block; width: 200px; height: 200px }
         frame { display: block; width: 100px; height: 100px; overflow: hidden;
                 border-radius: 8px; transform: translateX(10px) }
         a, b { display: block; width: 40px; height: 40px }
         .red { background: red }
         .blue { background: blue }",
        200.0,
        200.0,
    )
}

/// Swaps the classes of the element `name` and composes the change.
fn restyle(harness: &mut Harness, name: &str, from: &str, to: &str) {
    let element = harness.element(name);
    harness.edit_and_restyle(|edit| {
        edit.remove_class(element, ClassName::new(from));
        edit.add_class(element, ClassName::new(to));
    });
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [element]);
    harness.compose_from_marks(200.0, 200.0);
}

/// The holds the paint table counts over every paint.
fn paint_holds(harness: &Harness) -> u32 {
    (0..harness.scene.paints.slots() as u32)
        .filter_map(|raw| harness.scene.paints.refs(zgui_scene::PaintId(raw)))
        .sum()
}

#[test]
fn a_slot_another_chunk_still_names_stays() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = boxes();
    output.frame_everything(&mut harness);

    restyle(&mut harness, "a", "red", "blue");
    let written = output.frame(&mut harness);

    assert!(written.report.recorded > 0, "{written:?}");
    assert_eq!(written.report.released, 0, "{written:?}");
}

#[test]
fn a_slot_no_chunk_names_any_more_is_removed_and_its_paint_given_back() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = boxes();
    output.frame_everything(&mut harness);
    restyle(&mut harness, "a", "red", "blue");
    output.frame(&mut harness);
    let held = paint_holds(&harness);

    restyle(&mut harness, "b", "red", "blue");
    let written = output.frame(&mut harness);

    assert_eq!(written.report.released, 1, "the red slot: {written:?}");
    assert_eq!(paint_holds(&harness), held - 1);
}

#[test]
fn removing_a_box_removes_the_nodes_only_its_chunks_named() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = boxes();
    output.frame_everything(&mut harness);

    let frame = harness.element("frame");
    harness.edit_and_restyle(|edit| edit.remove(frame));
    harness.rebuild(200.0, 200.0);
    let written = output.frame(&mut harness);

    // The frame's clip, its coordinate system, and nothing the first box draws with.
    assert!(written.report.removed > 0, "{written:?}");
    assert_eq!(written.report.released, 2, "{written:?}");
    assert_eq!(output.frame(&mut harness).report.released, 0);
}

#[test]
fn a_reset_gives_the_mirror_holds_back_before_mirroring_again() {
    let Some(mut first) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = boxes();
    first.frame_everything(&mut harness);
    let held = paint_holds(&harness);

    harness
        .painter
        .compositor()
        .expect("attached by the first frame")
        .reset();
    let Some(mut second) = Recorded::new([200, 200]) else {
        return;
    };
    // The sync keeps drawing into the first output's id, which the second compositor made too.
    second.frame_everything(&mut harness);

    assert_eq!(paint_holds(&harness), held);
}
