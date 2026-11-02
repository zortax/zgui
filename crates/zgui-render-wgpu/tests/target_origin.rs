//! Pool targets that start away from the surface origin draw the same pixels.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::Size;
use zgui_render_wgpu::GroupPool;
use zgui_scene::{BackdropFilter, Filter, GroupBoundary, Quad, Scene};

use support::{SIDE, opaque, present, rect, renderer_pair};

/// How far the second rendering moves every pool target's origin.
const MARGIN: i32 = 6;

/// A quad of `color` filling `bounds`.
fn quad(scene: &mut Scene, bounds: (f32, f32, f32, f32), color: [u8; 3]) {
    let paint = scene.paints.add(zgui_scene::Paint::Solid(opaque(
        color[0], color[1], color[2],
    )));
    scene.push_quad(Quad::filled(
        rect(bounds.0, bounds.1, bounds.2, bounds.3),
        paint,
    ));
}

/// A group over `bounds` with `opacity` and `filters`.
fn group(bounds: (f32, f32, f32, f32), opacity: f32, filters: Vec<Filter>) -> GroupBoundary {
    GroupBoundary::start(
        rect(bounds.0, bounds.1, bounds.2, bounds.3),
        opacity,
        zgui_scene::peniko::BlendMode::default(),
        filters.into_iter().collect(),
    )
}

/// One frame that reads pool targets in every way the renderer does: a composite at opacity, a
/// map before a blur, a drop shadow, a backdrop over the composed target, and a backdrop inside a
/// group, which copies between two pool targets.
fn scene() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    quad(&mut scene, (0.0, 0.0, 64.0, SIDE as f32), [20, 40, 200]);

    let mapped = group(
        (4.0, 4.0, 40.0, 40.0),
        0.7,
        vec![Filter::Invert(1.0), Filter::Blur(3.0)],
    );
    scene.push_group(mapped.clone());
    quad(&mut scene, (8.0, 8.0, 30.0, 30.0), [250, 200, 10]);
    scene.push_group(mapped.end());

    let shadowed = group(
        (72.0, 8.0, 32.0, 32.0),
        1.0,
        vec![Filter::DropShadow {
            offset_x: 6.0,
            offset_y: 5.0,
            blur: 3.0,
            color: [0.0, 0.0, 0.0, 0.8],
        }],
    );
    scene.push_group(shadowed.clone());
    quad(&mut scene, (72.0, 8.0, 32.0, 32.0), [200, 30, 30]);
    scene.push_group(shadowed.end());

    scene.push_backdrop(BackdropFilter::new(
        rect(40.0, 52.0, 48.0, 24.0),
        [Filter::Blur(5.0), Filter::Saturate(1.6)]
            .into_iter()
            .collect(),
    ));

    let holder = group((8.0, 84.0, 112.0, 40.0), 0.9, Vec::new());
    scene.push_group(holder.clone());
    quad(&mut scene, (8.0, 84.0, 56.0, 40.0), [10, 160, 60]);
    scene.push_backdrop(BackdropFilter::new(
        rect(48.0, 92.0, 40.0, 24.0),
        [Filter::Blur(4.0)].into_iter().collect(),
    ));
    quad(&mut scene, (100.0, 100.0, 8.0, 8.0), [0, 0, 0]);
    scene.push_group(holder.end());

    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn a_target_away_from_the_surface_origin_draws_the_same_pixels() {
    let Some((mut renderer, mut moved)) = renderer_pair() else {
        return;
    };
    // Set both margins, so that a suite run in the test mode compares the same two renderings.
    renderer.set_target_margin(0);
    moved.set_target_margin(MARGIN);
    let guarded = SIDE + GroupPool::GUARD;
    assert_eq!(renderer.groups().covered(), Size::new(guarded, guarded));
    assert_eq!(
        moved.groups().covered(),
        Size::new(guarded + MARGIN, guarded + MARGIN),
        "every target starts {MARGIN} pixels above and to the left of the surface"
    );
    let reference = present(&mut renderer, &scene());
    let pixels = present(&mut moved, &scene());
    assert!(moved.groups().leases() > 0, "the frame isolated something");
    assert_eq!(
        reference.max_difference(&pixels),
        0,
        "moving the targets' origin moves no pixel"
    );
}
