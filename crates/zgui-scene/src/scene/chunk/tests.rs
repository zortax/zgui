//! Extracting a chunk out of the log, and replaying it frames later.

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};

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
    let replayed = scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(-8.0)));

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
    scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(0.0)));

    assert_eq!(scene.primitives.quads[1].order, 2);
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
