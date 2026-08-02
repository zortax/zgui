//! What building and finishing a scene records in the frame counters.
//!
//! The counter block is process-wide and accumulates until it is reset, so this lives in a test
//! binary of its own: any other test running beside it would add to the same numbers and the
//! assertions below would be about the whole binary rather than about one scene.
//!
//! These numbers are what a later budget is written against — "this interaction costs one vector
//! pass and culls nineteen items" is a sentence about counters. Without an assertion here, deleting
//! a `counter::add` call would leave every other test green and quietly turn every such budget into
//! a statement about zero.

use std::sync::Arc;

use kurbo::Shape;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Size, Vec2};
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_scene::{ClipLink, PaintRef, Quad, Scene, VectorId, VectorItem};

/// A device rectangle.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// A rectangular path covering `bounds`.
fn path(bounds: Rect<DevicePx, Device>) -> Arc<kurbo::BezPath> {
    Arc::new(
        kurbo::Rect::new(
            bounds.origin.x.0 as f64,
            bounds.origin.y.0 as f64,
            (bounds.origin.x.0 + bounds.size.width.0) as f64,
            (bounds.origin.y.0 + bounds.size.height.0) as f64,
        )
        .to_path(0.1),
    )
}

#[test]
fn a_finished_scene_records_what_it_emitted_culled_ordered_and_planned() {
    counter::reset();

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(256, 256));
    let fill = {
        let id = scene.paints.solid(Color::srgb(0.5, 0.5, 0.5, 1.0));
        PaintRef::solid(id)
    };

    let card = rect(0.0, 0.0, 256.0, 64.0);
    scene
        .push_quad(Quad::filled(card, fill))
        .expect("the card is on the surface");

    let clip = scene.clips.only(ClipLink::rect(card));
    assert!(
        scene
            .push_quad(Quad::filled(rect(0.0, 200.0, 16.0, 16.0), fill).clipped(clip))
            .is_none(),
        "a quad the card's clip admits nothing of is culled"
    );

    // Two avatars side by side, each with its own rounded clip inside the card's: one pass, and
    // one absorbed clip layer each.
    for index in 0..2u32 {
        let bounds = rect(8.0 + index as f32 * 64.0, 8.0, 48.0, 48.0);
        let rounded = scene
            .clips
            .push(clip, ClipLink::rounded(bounds, Vec2::splat(DevicePx(24.0))));
        scene
            .push_vector(VectorItem::filled(VectorId(index), path(bounds), fill).clipped(rounded))
            .expect("the avatars are on the surface");
    }

    scene.finish(&DamageSet::full());
    assert_eq!(scene.pass_plan().len(), 1);
    assert_eq!(scene.pass_plan().clip_layers, 2);

    if !COUNTERS_ENABLED {
        return;
    }
    let frame = counter::snapshot();
    assert_eq!(frame.primitives_emitted, 3, "one quad and two vector items");
    assert_eq!(frame.primitives_culled, 1, "the quad outside the card");
    assert_eq!(
        frame.bounds_tree_inserts, 3,
        "a culled primitive never reaches the bounds tree"
    );
    assert_eq!(counter::get(Counter::VelloPasses), 1);
    assert_eq!(counter::get(Counter::VectorClipLayers), 2);
}
