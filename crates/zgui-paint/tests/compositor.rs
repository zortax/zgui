//! Fragments recorded as compositor chunks.

mod support;

use support::{Element, Harness};

fn panels() -> Harness {
    Harness::sized(
        Element::new("root").children(vec![
            Element::new("a").classes(&["panel"]),
            Element::new("b").classes(&["panel"]),
        ]),
        "root { display: block; width: 200px; height: 100px; background: white }
         .panel { display: block; width: 80px; height: 30px; background: blue;
                  border: 2px solid red; border-radius: 4px }",
        200.0,
        100.0,
    )
}

#[test]
fn a_moved_fragment_writes_its_offset_once() {
    let mut harness = Harness::sized(
        Element::new("root").children(vec![
            Element::new("spacer"),
            Element::new("mark").classes(&["panel"]),
        ]),
        "root { display: block; width: 200px; height: 100px }
         spacer { display: block; height: 10px }
         .tall { height: 30px }
         .panel { display: block; width: 80px; height: 30px; background: blue }",
        200.0,
        100.0,
    );
    harness.paint_everything();

    let spacer = harness.element("spacer");
    harness.edit_and_restyle(|edit| edit.add_class(spacer, zgui_interned::ClassName::new("tall")));
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [spacer]);
    harness.compose_from_marks(200.0, 100.0);
    harness.paint();
    let moved = harness.compositor.commit;
    let drawn = harness.drawing().is_some();
    harness.paint();
    let again = harness.compositor.commit;

    assert!(drawn, "the move draws a frame");
    assert_eq!(moved.offsets_written, 1, "{moved:?}");
    assert_eq!(again.offsets_written, 0, "{again:?}");
}

#[test]
fn every_painted_quad_reaches_the_compositor_frame() {
    let mut harness = panels();
    harness.paint_everything();

    let sync = harness.compositor.sync;
    assert_eq!(sync.omitted_primitives, 0);
    assert!(sync.recorded >= 3, "{sync:?}");
    // The root's background, and each panel's background with its border on it.
    let drawing = harness.redraw();
    assert_eq!(drawing.quads().len(), 3, "{drawing}");
}

#[test]
fn a_repaint_that_changed_nothing_records_nothing_and_builds_no_frame() {
    let mut harness = panels();
    harness.paint_everything();

    harness.paint_everything();

    let sync = harness.compositor.sync;
    assert_eq!(sync.recorded + sync.offsets + sync.removed, 0, "{sync:?}");
    assert_eq!(sync.order_entries, 0);
    assert_eq!(sync.table_writes, 0);
    assert!(harness.drawing().is_none(), "{:?}", harness.drawing());
}

#[test]
fn a_fragment_rebuilt_in_its_boxs_place_records_over_its_chunk() {
    // A scroller whose content turns from tall to wide trades its vertical bar for a horizontal
    // one. The bar's pieces are new fragments in the places the old ones held among the box's.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![
            Element::new("scroller").children(vec![Element::new("content")]),
        ]),
        "root { display: block; width: 200px; height: 200px }
         scroller { display: block; width: 100px; height: 100px; overflow: auto }
         content { display: block; width: 50px; height: 300px }
         .wide { width: 300px; height: 50px }",
        200.0,
        200.0,
    );
    harness.paint_everything();
    let scroller = harness.box_of("scroller");
    let before = harness.store.fragments_of_box(scroller).to_vec();

    let content = harness.element("content");
    harness.edit_and_restyle(|edit| edit.add_class(content, zgui_interned::ClassName::new("wide")));
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [content]);
    harness.compose_from_marks(200.0, 200.0);
    let after = harness.store.fragments_of_box(scroller).to_vec();
    harness.paint();

    assert_eq!(
        before.len(),
        after.len(),
        "the fixture keeps the number of pieces"
    );
    assert!(
        before[1..].iter().all(|piece| !after.contains(piece)),
        "the fixture has to rebuild the bar's pieces: {before:?} {after:?}"
    );
    let commit = harness.compositor.commit;
    assert!(commit.chunks_recorded >= 2, "{commit:?}");
    assert_eq!(
        (commit.chunks_removed, commit.keys_spliced),
        (0, 0),
        "the new pieces are recorded over the old ones' chunks: {commit:?}"
    );
}
