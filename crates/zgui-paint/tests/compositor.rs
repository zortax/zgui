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
    assert!(commit.chunks_recorded >= 1, "{commit:?}");
    assert_eq!(
        (commit.chunks_removed, commit.keys_spliced),
        (0, 0),
        "the new pieces are recorded over the old ones' chunks: {commit:?}"
    );
}

#[test]
fn a_row_scrolled_out_of_its_port_does_not_stay_drawn_where_it_was() {
    // A port under a transform of its own scrolls by moving its rows. The first row's new place
    // is clipped away, so the damage never reaches it there; its chunk still draws at the old
    // place, which the damage does reach.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("moved").children(vec![
            Element::new("port").children(vec![
                Element::new("first").classes(&["row"]),
                Element::new("second").classes(&["row"]),
                Element::new("third").classes(&["row"]),
            ]),
        ])]),
        "root { display: block; width: 200px; height: 200px }
         moved { display: block; transform: translate(0px, 0px); overflow: hidden }
         port { display: block; width: 100px; height: 40px; overflow-y: scroll }
         .row { display: block; height: 20px; background: rgb(0, 0, 255) }
         first.row { background: rgb(255, 0, 0) }",
        200.0,
        200.0,
    );
    harness.paint_everything();
    let red = |harness: &mut Harness| {
        harness
            .redraw()
            .quads()
            .iter()
            .filter(|quad| quad.color("fill") == Some([1.0, 0.0, 0.0, 1.0]))
            .count()
    };
    assert_eq!(red(&mut harness), 1, "the control: the first row is drawn");

    harness.clear_damage();
    harness.scroll_to("port", 0.0, 30.0);
    harness.paint();

    let drawing = harness.redraw();
    for quad in drawing.quads() {
        if quad.color("fill") == Some([1.0, 0.0, 0.0, 1.0]) {
            let [_, y, _, height] = quad.bounds();
            let moved = quad
                .field("paint_origin")
                .is_some_and(|origin| origin != "[0, 0]");
            assert!(
                moved || y + height <= 0.0,
                "the first row is still drawn where it was: {drawing}"
            );
        }
    }
}

#[test]
fn a_row_a_plain_port_scrolls_away_is_not_visited() {
    // A plain port scrolls by moving its content space, so a row keeps its rectangle and its
    // chunk moves with the space. The walk does not visit a row the damage misses.
    let rows: Vec<Element> = ["first", "second", "third", "fourth", "fifth", "sixth"]
        .into_iter()
        .map(|name| Element::new(name).classes(&["row"]))
        .collect();
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("port").children(rows)]),
        "root { display: block; width: 200px; height: 200px }
         port { display: block; width: 100px; height: 40px; overflow-y: scroll }
         .row { display: block; height: 20px; background: rgb(0, 0, 255) }",
        200.0,
        200.0,
    );
    harness.paint_everything();
    let first = harness.fragment_of("first");

    harness.clear_damage();
    harness.scroll_to("port", 0.0, 80.0);
    let report = harness.paint();

    assert!(
        !report.emitted(first),
        "the first row was visited after it scrolled away"
    );
}

/// A port whose card straddles the bottom edge, with a red mark inside the card below the edge.
fn straddling_card() -> Harness {
    Harness::sized(
        Element::new("root").children(vec![Element::new("port").children(vec![
            Element::new("spacer"),
            Element::new("card").children(vec![Element::new("gap"), Element::new("mark")]),
            Element::new("tail"),
        ])]),
        "root { display: block; width: 200px; height: 100px }
         port { display: block; width: 200px; height: 100px; overflow-y: scroll }
         spacer { display: block; height: 50px }
         .tall { height: 300px }
         card { display: block; height: 100px }
         gap { display: block; height: 60px }
         mark { display: block; height: 20px; background: rgb(255, 0, 0) }
         tail { display: block; height: 600px }",
        200.0,
        100.0,
    )
}

/// How many red quads a whole redraw of `harness` draws.
fn red_quads(harness: &mut Harness) -> usize {
    harness
        .redraw()
        .quads()
        .iter()
        .filter(|quad| quad.color("fill") == Some([1.0, 0.0, 0.0, 1.0]))
        .count()
}

/// Replaces the damage with the band of the surface from `top` down by `height`.
fn damage(harness: &mut Harness, top: i32, height: i32) {
    harness.clear_damage();
    harness
        .damage
        .absorb(zgui_geom::Rect::<i32, zgui_geom::Device>::new(
            zgui_geom::Point::new(0, top),
            zgui_geom::Size::new(200, height),
        ));
}

#[test]
fn a_chunk_left_where_layout_moved_its_box_away_is_not_drawn() {
    // A reflow moves the card off the surface, and a scroll then brings the place the mark was
    // recorded at into view.
    let mut harness = straddling_card();
    harness.paint_everything();
    harness.scroll_to("port", 0.0, 100.0);
    harness.paint_everything();
    assert_eq!(red_quads(&mut harness), 1, "the control: the mark is drawn");
    harness.scroll_to("port", 0.0, 0.0);
    harness.paint_everything();

    let spacer = harness.element("spacer");
    harness.edit_and_restyle(|edit| edit.add_class(spacer, zgui_interned::ClassName::new("tall")));
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [spacer]);
    harness.compose_from_marks(200.0, 100.0);
    damage(&mut harness, 50, 50);
    harness.paint();
    harness.scroll_to("port", 0.0, 30.0);
    damage(&mut harness, 70, 30);
    harness.paint();

    assert_eq!(
        red_quads(&mut harness),
        0,
        "the mark is drawn where it was before the reflow"
    );
}

#[test]
fn a_chunk_left_where_layout_moved_it_inside_a_skipped_box_is_not_drawn() {
    // The mark moves inside the panel, whose own ink covers both places, so the panel's subtree
    // ink stays the same while the walk skips it. A band of damage then reaches the place the
    // mark was recorded at, and only the panel's own fragment there.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![
            Element::new("spacer"),
            Element::new("panel").children(vec![
                Element::new("holder").children(vec![Element::new("mark")]),
            ]),
        ]),
        "root { display: block; width: 200px; height: 200px }
         spacer { display: block; height: 50px }
         panel { display: block; height: 100px; background: rgb(255, 255, 255) }
         holder { display: block; height: 20px }
         mark { display: block; position: relative; top: 60px; height: 20px;
                background: rgb(255, 0, 0) }
         .up { top: 0px }",
        200.0,
        200.0,
    );
    harness.paint_everything();
    assert_eq!(red_quads(&mut harness), 1, "the control: the mark is drawn");

    let mark = harness.element("mark");
    harness.edit_and_restyle(|edit| edit.add_class(mark, zgui_interned::ClassName::new("up")));
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [mark]);
    harness.compose_from_marks(200.0, 200.0);
    damage(&mut harness, 0, 10);
    harness.paint();
    damage(&mut harness, 110, 20);
    harness.paint();

    let drawing = harness.redraw();
    for quad in drawing.quads() {
        if quad.color("fill") == Some([1.0, 0.0, 0.0, 1.0]) {
            let [_, y, _, _] = quad.bounds();
            assert!(y < 100.0, "the mark is drawn where it was: {drawing}");
        }
    }
}
