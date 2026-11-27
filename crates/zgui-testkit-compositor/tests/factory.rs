//! The factory's compositor and renderer draw a committed chunk.

use iceblink::{
    Anchor, BufferAge, Built, ChunkFlags, ChunkKey, ChunkProps, ClipRef, Color, CornerShapes,
    Frame, FrameClock, FrameOutcome, FrameRequest, Matrix4, OrderEntry, OrderNode, OutputConfig,
    Paint, PaintKind, PaintRef, Quad, RenderTarget, SpaceRef, TransformNode,
};

#[test]
fn a_recorded_quad_is_built_into_a_frame_and_drawn() {
    let (mut compositor, mut renderer) = match (
        zgui_testkit_compositor::compositor(),
        zgui_testkit_compositor::renderer([64, 64]),
    ) {
        (Ok(compositor), Ok(renderer)) => (compositor, renderer),
        (Err(why), _) | (_, Err(why)) => {
            eprintln!("skipped: {why}");
            return;
        }
    };
    renderer.configure(RenderTarget {
        size: [64, 64],
        scale: 1.0,
        opaque: true,
    });
    let (client, _ids) = compositor.register_client();
    let output = compositor.create_output(OutputConfig {
        size: [64, 64],
        scale: 1.0,
        opaque: true,
    });

    let mut tx = compositor.transaction(client);
    let space = tx.create_transform(TransformNode {
        parent: None,
        local: Matrix4::IDENTITY,
        scroll: None,
        anchor: Anchor::Scrolls,
    });
    let root = tx.create_order(OrderNode::default());
    let paint = tx.create_paint(Paint::Solid(Color([0.0, 0.0, 1.0, 1.0])));
    let key = ChunkKey::new(1, 0);
    let mut chunk = tx.record(
        key,
        ChunkProps {
            space,
            clip: None,
            flags: ChunkFlags::default(),
        },
    );
    let fill = chunk.paint_ref(PaintKind::SOLID, paint);
    chunk.quad(Quad {
        bounds: [8.0, 8.0, 32.0, 32.0],
        radii: [0.0; 8],
        shapes: CornerShapes::default(),
        border: [0.0; 4],
        fill,
        stroke: PaintRef::NONE,
        paint_origin: [0.0; 2],
        clip: ClipRef::INHERIT,
        space: SpaceRef::INHERIT,
        style: Default::default(),
        _pad: [0; 3],
    });
    tx.splice(root, 0, 0, &[OrderEntry::Chunk(key)]);
    tx.set_root(output, Some(root));
    compositor.commit(tx).expect("the transaction commits");

    let caps = renderer.capabilities();
    let request = FrameRequest {
        caps: &caps,
        age: BufferAge::Kept,
        clock: FrameClock::default(),
        subpixel: None,
    };
    let mut frame = Frame::default();
    assert_eq!(
        compositor.build_frame(output, &request, &mut frame),
        Built::Frame
    );
    let outcome = renderer.draw(&frame);
    compositor.frame_done(output, frame.id, &outcome);
    assert!(
        matches!(outcome, FrameOutcome::Presented(_)),
        "the frame was not presented: {outcome:?}"
    );
    assert_eq!(compositor.counters().frame.items_drawn, 1);
}
