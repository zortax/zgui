//! Extracting a chunk out of the log, and replaying it frames later.

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::clip::ClipLink;
use crate::paint::PaintRef;
use crate::prim::{Decoration, DecorationStyle, Quad};
use crate::scene::Scene;
use crate::scene::chunk::ChunkPrims;

/// A device rectangle.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// A scene over a small surface, with one solid paint interned.
fn scene() -> (Scene, PaintRef) {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(400, 400));
    let id = scene.paints.solid(Color::srgb(1.0, 0.0, 0.0, 1.0));
    let fill = PaintRef::solid(id);
    (scene, fill)
}

#[test]
fn an_extracted_chunk_replays_the_same_primitives_translated() {
    let (mut scene, fill) = scene();
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 20.0, 20.0), fill));
    scene.push_quad(Quad::filled(rect(0.0, 40.0, 20.0, 20.0), fill));
    scene.push_decoration(Decoration::new(
        rect(0.0, 30.0, 20.0, 2.0),
        2.0,
        Color::srgb(0.0, 0.0, 0.0, 1.0),
        DecorationStyle::Solid,
    ));
    let mut chunk = ChunkPrims::default();
    scene.extract_chunk(0..scene.ops().len() as u32, &mut chunk);
    scene.finish(&DamageSet::full());

    // Two frame boundaries: the chunk stays valid where a log range from two frames ago is gone.
    scene.begin_frame(Size::new(400, 400));
    scene.begin_frame(Size::new(400, 400));
    let replayed = scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(-8.0)), 0);

    assert_eq!(replayed.len(), 3);
    assert_eq!(scene.primitives.quads.len(), 2);
    assert_eq!(scene.primitives.decorations.len(), 1);
    assert_eq!(scene.primitives.quads[0].bounds, [0.0, -8.0, 20.0, 20.0]);
    assert_eq!(scene.primitives.quads[1].bounds, [0.0, 32.0, 20.0, 20.0]);
    assert_eq!(
        scene.primitives.decorations[0].bounds,
        [0.0, 22.0, 20.0, 2.0]
    );
}

#[test]
fn a_replayed_chunk_is_ordered_against_this_frames_neighbours() {
    let (mut scene, fill) = scene();
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 20.0, 20.0), fill));
    let mut chunk = ChunkPrims::default();
    scene.extract_chunk(0..1, &mut chunk);
    scene.finish(&DamageSet::full());

    scene.begin_frame(Size::new(400, 400));
    // Something new is drawn underneath before the replay, so the replayed quad steps above it.
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 20.0, 20.0), fill));
    scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(0.0)), 0);

    assert_eq!(scene.primitives.quads[1].order, 2);
}

/// A replay keeps the order its primitives had against each other, and asks the tree once.
///
/// The three quads below overlap, so each stands above the one before it. Replayed, they have to
/// keep standing that way: it is the same picture, moved. What must not happen is what the ordinary
/// push path does — rediscover each of those orders by querying the tree, which is the largest
/// single cost in a scroll frame and answers a question nothing asked.
#[test]
fn a_replay_carries_its_own_ordering() {
    let (mut scene, fill) = scene();
    scene.begin_chunk_capture(ChunkPrims::default());
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 20.0, 20.0), fill));
    scene.push_quad(Quad::filled(rect(5.0, 5.0, 20.0, 20.0), fill));
    scene.push_quad(Quad::filled(rect(10.0, 10.0, 20.0, 20.0), fill));
    let chunk = scene.take_chunk_capture();
    assert_eq!(chunk.orders, vec![1, 2, 3], "the fixture has to be a stack");
    assert_eq!(chunk.span, 3);
    scene.finish(&DamageSet::full());

    scene.begin_frame(Size::new(400, 400));
    scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(0.0)), 0);
    let replayed: Vec<_> = scene.primitives.quads.iter().map(|q| q.order).collect();
    assert_eq!(replayed, vec![1, 2, 3], "a replay reordered its own primitives");
}

/// The order is the chunk's own, not the order the encoding frame happened to give it.
///
/// A fragment encoded over a busy page takes high orders; the same fragment replayed onto an empty
/// one takes low ones, and what has to survive is the difference between its members rather than
/// the numbers themselves. Carrying the frame's numbers forward would put the chunk wherever it
/// happened to be the day it was encoded.
#[test]
fn a_chunks_orders_are_its_own_and_not_the_frames() {
    let (mut scene, fill) = scene();
    // Ten overlapping quads first, so the frame's orders are nowhere near one.
    for index in 0..10 {
        let at = index as f32;
        scene.push_quad(Quad::filled(rect(at, at, 40.0, 40.0), fill));
    }
    scene.begin_chunk_capture(ChunkPrims::default());
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 20.0, 20.0), fill));
    scene.push_quad(Quad::filled(rect(5.0, 5.0, 20.0, 20.0), fill));
    let chunk = scene.take_chunk_capture();

    assert!(
        scene.primitives.quads.last().is_some_and(|q| q.order > 3),
        "the fixture has to put the frame's orders well above the chunk's"
    );
    assert_eq!(chunk.orders, vec![1, 2], "the chunk counts from its own floor");
}

/// A primitive the encoding culled still carries an order, and takes it when a replay admits it.
///
/// This is the case the frame's own order cannot answer for: the capture is the pushing's complete
/// content, so a chunk holds shapes the encoding clipped away — a row half outside a scroll port —
/// and those never reached the frame's tree at all. They are ordered against their chunk-mates all
/// the same, which is what lets the row be drawn whole two frames later.
#[test]
fn a_primitive_the_encoding_culled_is_still_ordered_among_its_chunk() {
    let (mut scene, fill) = scene();
    let port = scene
        .clips
        .only(ClipLink::rect(rect(0.0, 0.0, 400.0, 40.0)));

    scene.begin_chunk_capture(ChunkPrims::default());
    let inside = scene.push_quad(Quad::filled(rect(0.0, 0.0, 20.0, 20.0), fill).clipped(port));
    // Below the port, so this frame's clip refuses it outright.
    let outside = scene.push_quad(Quad::filled(rect(0.0, 100.0, 20.0, 20.0), fill).clipped(port));
    let chunk = scene.take_chunk_capture();

    assert!(inside.is_some());
    assert!(outside.is_none(), "the fixture has to have something culled");
    assert_eq!(chunk.ops.len(), 2, "the capture keeps what the frame refused");
    assert_eq!(chunk.orders.len(), 2, "and orders it too");
    scene.finish(&DamageSet::full());

    // Replayed far enough up that the port now falls on the shape the encoding refused — and off
    // the one it admitted, which is what a scroll does to a row.
    scene.begin_frame(Size::new(400, 400));
    scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(-90.0)), 0);
    let drawn: Vec<_> = scene
        .primitives
        .quads
        .iter()
        .map(|quad| (quad.bounds[1], quad.order))
        .collect();
    // Order one, and not two: the two shapes do not overlap, so the chunk's own tree gave them the
    // same order — which is the rule the frame's tree follows as well, and the reason a row of
    // cells costs one order rather than one each.
    assert_eq!(
        drawn,
        vec![(10.0, 1)],
        "the shape the encoding clipped away was not drawn, or lost the order it was captured with"
    );
}

/// Anything drawn over a replayed chunk still sorts above all of it.
///
/// The chunk is one leaf in the tree carrying its highest order, so this is coarser than asking per
/// primitive — a quad over the *bottom* of the stack sorts above the top of it as well. Coarse in
/// the safe direction: an order that comes out higher than it needed to be costs batching, and one
/// that comes out lower is a primitive drawn underneath something it was painted after.
#[test]
fn a_primitive_over_any_part_of_a_replayed_chunk_sorts_above_all_of_it() {
    let (mut scene, fill) = scene();
    scene.begin_chunk_capture(ChunkPrims::default());
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 20.0, 20.0), fill));
    scene.push_quad(Quad::filled(rect(5.0, 5.0, 20.0, 20.0), fill));
    scene.push_quad(Quad::filled(rect(10.0, 10.0, 20.0, 20.0), fill));
    let chunk = scene.take_chunk_capture();
    scene.finish(&DamageSet::full());

    scene.begin_frame(Size::new(400, 400));
    scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(0.0)), 0);
    let top = scene
        .primitives
        .quads
        .iter()
        .map(|quad| quad.order)
        .max()
        .expect("the chunk replayed");

    // Overlapping only the lowest quad of the stack, and nothing else.
    let over = scene.push_quad(Quad::filled(rect(0.0, 0.0, 3.0, 3.0), fill));
    assert!(
        over.is_some_and(|order| order > top),
        "a quad drawn over a replayed chunk sorted inside it: {over:?} against {top}"
    );

    // And something disjoint from it still reuses a low order, so the block is a rectangle rather
    // than a barrier.
    let elsewhere = scene.push_quad(Quad::filled(rect(300.0, 300.0, 10.0, 10.0), fill));
    assert_eq!(elsewhere, Some(1));
}

/// What the ordering is worth, counted.
///
/// A scroll frame's emit walk is three quarters of the frame, and the largest symbol inside it is
/// the draw-order tree taking one insert per primitive of the port — including every primitive of
/// every row that only moved. One entry per replayed *chunk* is what a row costs instead.
#[test]
fn a_replay_costs_one_tree_entry_however_many_primitives_it_holds() {
    const PRIMITIVES: usize = 64;
    let (mut scene, fill) = scene();
    scene.begin_chunk_capture(ChunkPrims::default());
    for index in 0..PRIMITIVES {
        let at = index as f32;
        scene.push_quad(Quad::filled(rect(at, at, 20.0, 20.0), fill));
    }
    let chunk = scene.take_chunk_capture();
    scene.finish(&DamageSet::full());

    scene.begin_frame(Size::new(400, 400));
    scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(-8.0)), 0);

    // The tree's own leaves, rather than the frame counters: those are a process-wide store and
    // these tests run beside one another.
    let leaves = scene.order.len();
    assert_eq!(scene.primitives.quads.len(), PRIMITIVES, "all of them replayed");
    assert_eq!(
        leaves, 1,
        "{PRIMITIVES} primitives that had already been ordered cost {leaves} entries in the \
         draw-order tree"
    );
}

#[test]
fn extraction_rebases_indices_to_the_chunks_own_arrays() {
    let (mut scene, fill) = scene();
    // Three quads pushed, and only the range of the last one extracted: its frame index is 2, so
    // a chunk that kept the frame index would replay a different quad or nothing.
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 10.0, 10.0), fill));
    scene.push_quad(Quad::filled(rect(20.0, 0.0, 10.0, 10.0), fill));
    scene.push_quad(Quad::filled(rect(40.0, 0.0, 10.0, 10.0), fill));
    let mut chunk = ChunkPrims::default();
    scene.extract_chunk(2..3, &mut chunk);

    assert_eq!(chunk.quads.len(), 1);
    assert_eq!(chunk.ops[0].index, 0);
    assert_eq!(chunk.quads[0].bounds, [40.0, 0.0, 10.0, 10.0]);
}

#[test]
fn an_out_of_bounds_range_extracts_nothing() {
    let (mut scene, fill) = scene();
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 10.0, 10.0), fill));
    let mut chunk = ChunkPrims::default();
    scene.extract_chunk(0..99, &mut chunk);
    assert!(chunk.is_empty());
}

#[test]
fn clearing_a_chunk_keeps_its_allocations() {
    let (mut scene, fill) = scene();
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 10.0, 10.0), fill));
    let mut chunk = ChunkPrims::default();
    scene.extract_chunk(0..1, &mut chunk);
    let bytes = chunk.bytes();
    chunk.clear();
    assert!(chunk.is_empty());
    assert_eq!(chunk.bytes(), bytes);
}

/// A clip the encoding minted travels with the chunk instead of staying where it was drawn.
///
/// The `text-overflow` window is the case: an ellipsized line's chunk carries every glyph, and the
/// window is the whole of what cuts it. Left at the encode position, the window culls the glyphs
/// of every moved replay — the visible slice shrinks as the scroll runs, and past one line height
/// only the mark is left.
#[test]
fn a_minted_clip_is_re_interned_where_the_chunk_replays() {
    let (mut scene, fill) = scene();
    scene.begin_chunk_capture(ChunkPrims::default());
    let window = scene.clips.only(ClipLink::rect(rect(0.0, 0.0, 30.0, 20.0)));
    scene.note_minted_clip(window);
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 20.0, 20.0), fill).clipped(window));
    let chunk = scene.take_chunk_capture();
    assert_eq!(chunk.minted, vec![window], "the capture holds the note");
    scene.finish(&DamageSet::full());

    // Moved past its own height, so an encode-position window would refuse the quad outright.
    scene.begin_frame(Size::new(400, 400));
    let replayed = scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(40.0)), 0);
    assert_eq!(replayed.len(), 1, "the moved window admits the moved quad");
    assert_eq!(scene.clips.bounds(window), rect(0.0, 40.0, 30.0, 20.0));

    // Back in place: the same slot holds the encode rectangle again.
    scene.finish(&DamageSet::full());
    scene.begin_frame(Size::new(400, 400));
    let returned = scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(0.0)), 0);
    assert_eq!(returned.len(), 1);
    assert_eq!(scene.clips.bounds(window), rect(0.0, 0.0, 30.0, 20.0));
}
