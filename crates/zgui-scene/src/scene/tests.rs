//! Building a scene: the clip cull, the log, batching and replay.

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::batch::Batch;
use crate::clip::ClipLink;
use crate::id::ClipId;
use crate::paint::PaintRef;
use crate::prim::{Decoration, DecorationStyle, PrimitiveKind, Quad};
use crate::scene::Scene;

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
fn a_primitive_its_clip_admits_nothing_of_never_reaches_the_display_list() {
    let (mut scene, fill) = scene();
    let scrollport = scene
        .clips
        .only(ClipLink::rect(rect(0.0, 0.0, 400.0, 100.0)));

    let visible =
        scene.push_quad(Quad::filled(rect(0.0, 10.0, 50.0, 20.0), fill).clipped(scrollport));
    let offscreen =
        scene.push_quad(Quad::filled(rect(0.0, 300.0, 50.0, 20.0), fill).clipped(scrollport));

    assert!(visible.is_some());
    assert!(offscreen.is_none());
    assert_eq!(scene.primitives.quads.len(), 1);
    assert_eq!(scene.ops().len(), 1);
}

#[test]
fn a_group_marker_is_never_culled_and_takes_an_order_above_everything() {
    use smallvec::smallvec;

    use crate::group::{Filter, GroupBoundary};

    let (mut scene, fill) = scene();
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 50.0, 50.0), fill));

    let empty_clip = scene
        .clips
        .only(ClipLink::rect(rect(1000.0, 1000.0, 1.0, 1.0)));
    let group = GroupBoundary::start(
        rect(0.0, 0.0, 50.0, 50.0),
        0.5,
        peniko::BlendMode::default(),
        smallvec![Filter::Blur(2.0)],
    )
    .clipped(empty_clip);

    let start = scene.push_group(group.clone());
    assert_eq!(start, 2, "markers sort above everything already pushed");

    let end = scene.push_group(group.end());
    assert_eq!(scene.primitives.groups.len(), 2);

    // Everything after the group closes sits *strictly* above it, however far away it is drawn.
    //
    // Strictly, and not merely at the same order. Equal draw order is settled by `PrimitiveKind`,
    // which puts a closing marker last, so a quad free to take the marker's own order sorts ahead
    // of it — inside a group that has already finished, and clipped away by that group's bounds.
    // The quad below is disjoint from the group, which is exactly the case the floor exists for:
    // an overlapping one would have been pushed above it by the ordinary query.
    let after = scene
        .push_quad(Quad::filled(rect(300.0, 300.0, 10.0, 10.0), fill))
        .unwrap();
    assert!(
        after > end,
        "a primitive drawn after a group closed took the closing marker's own order ({end}), so \
         it is drawn into that group's target"
    );
}

#[test]
fn a_layer_forces_its_order_on_everything_inside_it() {
    let (mut scene, fill) = scene();
    scene.push_layer(500);
    let first = scene.push_quad(Quad::filled(rect(0.0, 0.0, 10.0, 10.0), fill));
    let second = scene.push_quad(Quad::filled(rect(0.0, 0.0, 10.0, 10.0), fill));
    scene.pop_layer();
    assert_eq!(first, Some(500));
    assert_eq!(second, Some(500));

    let outside = scene.push_quad(Quad::filled(rect(200.0, 200.0, 10.0, 10.0), fill));
    assert_eq!(outside, Some(1), "the layer no longer applies");
}

#[test]
fn finishing_sorts_the_arrays_and_keeps_the_log_pointing_at_the_right_primitives() {
    let (mut scene, fill) = scene();
    // Pushed in an order that does not match their draw order: the second overlaps the first, and
    // the third is disjoint from both.
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 40.0, 40.0), fill));
    scene.push_quad(Quad::filled(rect(20.0, 20.0, 40.0, 40.0), fill));
    scene.push_quad(Quad::filled(rect(300.0, 300.0, 40.0, 40.0), fill));

    let before: Vec<_> = scene
        .ops()
        .iter()
        .map(|op| scene.primitives.quads[op.index as usize].bounds)
        .collect();

    scene.finish(&DamageSet::full());

    let after: Vec<_> = scene
        .ops()
        .iter()
        .map(|op| scene.primitives.quads[op.index as usize].bounds)
        .collect();
    assert_eq!(
        before, after,
        "a recorded log entry must name the same primitive before and after the sort"
    );

    let orders: Vec<_> = scene
        .primitives
        .quads
        .iter()
        .map(|quad| quad.order)
        .collect();
    assert!(orders.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn batches_merge_by_order_and_break_where_another_kind_has_to_come_first() {
    let (mut scene, fill) = scene();
    // Two disjoint quads at order 1, a decoration over one of them at order 2, then a quad over
    // the decoration at order 3.
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 20.0, 20.0), fill));
    scene.push_quad(Quad::filled(rect(100.0, 0.0, 20.0, 20.0), fill));
    scene.push_decoration(Decoration::new(
        rect(0.0, 10.0, 20.0, 2.0),
        2.0,
        Color::srgb(0.0, 0.0, 0.0, 1.0),
        DecorationStyle::Solid,
    ));
    scene.push_quad(Quad::filled(rect(0.0, 10.0, 20.0, 4.0), fill));

    scene.finish(&DamageSet::full());
    let batches: Vec<_> = scene.batches().collect();

    assert_eq!(
        batches,
        vec![
            Batch::Quads(0..2),
            Batch::Decorations(0..1),
            Batch::Quads(2..3),
        ]
    );
}

#[test]
fn the_side_tables_survive_a_frame_boundary_but_the_primitives_do_not() {
    let (mut scene, _) = scene();
    let clip = scene.clips.only(ClipLink::rect(rect(0.0, 0.0, 10.0, 10.0)));
    let paint = scene.paints.solid(Color::srgb(0.0, 1.0, 0.0, 1.0));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, 5.0, 5.0),
        PaintRef::solid(paint),
    ));
    scene.finish(&DamageSet::full());

    scene.begin_frame(Size::new(400, 400));
    assert!(scene.primitives.is_empty());
    assert!(scene.clips.contains(clip));
    assert!(scene.paints.contains(paint));
    assert!(scene.clips.contains(ClipId::ROOT));
}

#[test]
#[should_panic(expected = "batches() needs a finished scene")]
fn batching_an_unfinished_scene_is_refused_rather_than_producing_a_wrong_sequence() {
    let (scene, _) = scene();
    let _ = scene.batches().count();
}

#[test]
fn the_log_records_every_kind_it_can_hold() {
    let (mut scene, fill) = scene();
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 10.0, 10.0), fill));
    scene.push_decoration(Decoration::new(
        rect(0.0, 20.0, 10.0, 2.0),
        2.0,
        Color::srgb(0.0, 0.0, 0.0, 1.0),
        DecorationStyle::Solid,
    ));
    let kinds: Vec<_> = scene.ops().iter().map(|op| op.kind).collect();
    assert_eq!(kinds, vec![PrimitiveKind::Quad, PrimitiveKind::Decoration]);
}

/// A primitive drawn in a space of its own survives a clip that was measured in another one.
///
/// The failure this stands against is a whole control disappearing: a dialog laid out past the
/// right edge of the window and moved back inside it by a transform, whose trailing keys are
/// recorded beyond the window's clip rectangle and drawn well inside it. The cull is an
/// optimisation and the shader is the authority, so a link from a space that has moved out from
/// under the ink is left to the shader.
#[test]
fn a_clip_from_another_space_does_not_cull() {
    use zgui_geom::Matrix4;

    use crate::spatial::{OwnSpace, PropertyOwner};

    let (mut scene, fill) = scene();
    let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
    let placed = Matrix4::translation(-450.0, 0.0, 0.0);
    let viewport = scene.spatial.viewport();
    let panel = scene
        .spatial
        .space_of(viewport, owner, OwnSpace::of(Some(placed), None, false));
    let window = scene
        .clips
        .only(ClipLink::rect(rect(0.0, 0.0, 400.0, 400.0)));

    // Laid out past the window's right edge, drawn 450 to the left of where it was laid out.
    let key = rect(500.0, 40.0, 30.0, 30.0);
    let mut quad = Quad::filled(key, fill).clipped(window);
    quad.transform = panel.index();
    let placed_key = scene.push_quad(quad);
    // The control: the same rectangle drawn where it was laid out is outside the window and gone.
    let left_behind = scene.push_quad(Quad::filled(key, fill).clipped(window));

    assert!(placed_key.is_some(), "the key the transform brings back in");
    assert!(left_behind.is_none(), "the one nothing moves");
    assert_eq!(scene.primitives.quads.len(), 1);
}
