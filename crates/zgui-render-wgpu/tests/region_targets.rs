//! Pool targets cover the region a group or a filter needs, and no more.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{Device, Point, Rect, Scale, Size};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, GroupPool, Pixels, WgpuRenderer, wgpu};
use zgui_scene::{BackdropFilter, Filter, GroupBoundary, Quad, Scene};

use support::{device_lock, opaque, rect};

/// The extent every surface here has: larger than one size class, so that a target cut to a
/// region is smaller than one over the whole surface.
const EXTENT: i32 = 512;

/// Two renderers of `EXTENT` on one device, and the device lock they hold.
fn pair() -> Option<(
    std::sync::MutexGuard<'static, ()>,
    WgpuRenderer,
    WgpuRenderer,
)> {
    let guard = device_lock();
    let make = || {
        let target = RenderTarget::new(Size::new(EXTENT, EXTENT), Scale::new(1.0));
        Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)
    };
    match (make(), make()) {
        (Ok(first), Ok(second)) => Some((guard, first, second)),
        (Err(failure), _) | (_, Err(failure)) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            None
        }
    }
}

/// Draws `scene` against `damage` and reads back the result.
fn draw(renderer: &mut WgpuRenderer, scene: &Scene, damage: &DamageSet) -> Pixels {
    let outcome = renderer.draw(scene, damage);
    assert!(
        outcome.stats().is_some(),
        "the frame was composed: {outcome:?}"
    );
    renderer
        .read_presented()
        .expect("a stand-in surface can be read back")
}

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

/// A whole-pixel rectangle.
fn area(x: i32, y: i32, width: i32, height: i32) -> Rect<i32, Device> {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

/// How many bytes one isolated target of `side` device pixels square takes, after rounding.
fn target_bytes(side: i32) -> u64 {
    let allocated = GroupPool::new(Size::new(side, side), 0)
        .allocated_extent(zgui_render_wgpu::TargetScale::Full);
    u64::from(GroupPool::FORMAT.block_copy_size(None).unwrap_or(8))
        * allocated.width as u64
        * allocated.height as u64
}

/// The largest difference between two readbacks inside `within`.
fn difference_in(first: &Pixels, second: &Pixels, within: Rect<i32, Device>) -> u8 {
    let mut largest = 0;
    for y in within.top()..within.bottom() {
        for x in within.left()..within.right() {
            let (a, b) = (first.rgba(x, y), second.rgba(x, y));
            for channel in 0..4 {
                largest = largest.max(a[channel].abs_diff(b[channel]));
            }
        }
    }
    largest
}

/// Groups whose filters read past the rectangles [`damage`] redraws: a blur beside one, a drop
/// shadow cast into one, and a backdrop in a blurred group beside one.
fn filtered() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(EXTENT, EXTENT));
    quad(
        &mut scene,
        (0.0, 0.0, EXTENT as f32, EXTENT as f32),
        [255; 3],
    );

    let blurred = group((20.0, 20.0, 40.0, 40.0), 1.0, vec![Filter::Blur(6.0)]);
    scene.push_group(blurred.clone());
    quad(&mut scene, (20.0, 20.0, 40.0, 40.0), [200, 20, 20]);
    scene.push_group(blurred.end());

    let shadowed = group(
        (20.0, 100.0, 30.0, 30.0),
        1.0,
        vec![Filter::DropShadow {
            offset_x: 12.0,
            offset_y: 6.0,
            blur: 3.0,
            color: [0.0, 0.0, 0.0, 0.9],
        }],
    );
    scene.push_group(shadowed.clone());
    quad(&mut scene, (20.0, 100.0, 30.0, 30.0), [20, 160, 40]);
    scene.push_group(shadowed.end());

    // The group's blur reads the backdrop's pixels next to the rectangle, and the backdrop reads
    // the group's content far past them: white up to x = 180, black after.
    let holder = group((100.0, 200.0, 160.0, 100.0), 1.0, vec![Filter::Blur(4.0)]);
    scene.push_group(holder.clone());
    quad(&mut scene, (100.0, 200.0, 80.0, 100.0), [250, 250, 250]);
    quad(&mut scene, (180.0, 200.0, 80.0, 100.0), [0, 0, 0]);
    scene.push_backdrop(BackdropFilter::new(
        rect(150.0, 220.0, 60.0, 40.0),
        [Filter::Blur(12.0)].into_iter().collect(),
    ));
    scene.push_group(holder.end());

    scene.finish(&DamageSet::full());
    scene
}

/// Rectangles beside each group of [`filtered`], and one that misses every group.
fn damage() -> Vec<Rect<i32, Device>> {
    vec![
        area(64, 20, 16, 40),
        area(52, 100, 20, 40),
        area(130, 230, 18, 10),
        area(400, 400, 16, 16),
    ]
}

#[test]
fn a_partial_redraw_of_filtered_groups_draws_the_pixels_a_whole_one_does() {
    let Some((_guard, mut whole, mut partial)) = pair() else {
        return;
    };
    let reference = draw(&mut whole, &filtered(), &DamageSet::full());

    // Something else first, so that a pixel the partial redraw leaves alone shows.
    let mut other = Scene::new();
    other.begin_frame(Size::new(EXTENT, EXTENT));
    quad(
        &mut other,
        (0.0, 0.0, EXTENT as f32, EXTENT as f32),
        [10, 10, 90],
    );
    other.finish(&DamageSet::full());
    draw(&mut partial, &other, &DamageSet::full());

    let mut damaged = DamageSet::new();
    for rect in damage() {
        damaged.absorb(rect);
    }
    let leases = partial.groups().leases();
    let pixels = draw(&mut partial, &filtered(), &damaged);
    assert!(
        partial.groups().leases() > leases,
        "the partial frame isolated something"
    );
    for rect in damage() {
        assert_eq!(
            difference_in(&reference, &pixels, rect),
            0,
            "a partial redraw differs from a whole one inside {rect:?}"
        );
    }
}

/// One frame of a small group at `x` with `width`, over a white surface.
fn small_group(x: f32, width: f32) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(EXTENT, EXTENT));
    quad(
        &mut scene,
        (0.0, 0.0, EXTENT as f32, EXTENT as f32),
        [255; 3],
    );
    let faded = group((x, 40.0, width, 40.0), 0.5, Vec::new());
    scene.push_group(faded.clone());
    quad(&mut scene, (x, 40.0, width, 40.0), [0, 0, 200]);
    scene.push_group(faded.end());
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn a_small_group_takes_a_target_of_its_own_size_class() {
    let Some((_guard, mut renderer, _)) = pair() else {
        return;
    };
    draw(&mut renderer, &small_group(40.0, 40.0), &DamageSet::full());
    assert!(renderer.groups().leases() > 0);
    assert_eq!(
        renderer.groups().bytes(),
        target_bytes(1),
        "one target of the smallest size class"
    );
    assert!(target_bytes(1) * 4 <= target_bytes(EXTENT));
}

#[test]
fn a_group_over_the_whole_surface_takes_a_target_cut_to_the_damage() {
    let Some((_guard, mut renderer, _)) = pair() else {
        return;
    };
    let scene = || {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(EXTENT, EXTENT));
        let faded = group((0.0, 0.0, EXTENT as f32, EXTENT as f32), 0.5, Vec::new());
        scene.push_group(faded.clone());
        quad(
            &mut scene,
            (0.0, 0.0, EXTENT as f32, EXTENT as f32),
            [0, 0, 200],
        );
        scene.push_group(faded.end());
        scene.finish(&DamageSet::full());
        scene
    };
    draw(&mut renderer, &scene(), &DamageSet::full());
    assert_eq!(renderer.groups().bytes(), target_bytes(EXTENT));

    // Drop the whole-surface target, then redraw a small rectangle.
    renderer.set_group_budget(GroupPool::BUDGET);
    let mut small = DamageSet::new();
    small.absorb(area(200, 200, 16, 16));
    draw(&mut renderer, &scene(), &small);
    assert_eq!(
        renderer.groups().bytes(),
        target_bytes(1),
        "the target covers the damaged rectangle and its guard"
    );
}

#[test]
fn a_group_that_moves_and_grows_inside_its_size_class_allocates_nothing() {
    let Some((_guard, mut renderer, _)) = pair() else {
        return;
    };
    draw(&mut renderer, &small_group(40.0, 40.0), &DamageSet::full());
    let allocations = renderer.groups().allocations();
    assert!(allocations > 0);
    for frame in 1..=30 {
        let step = frame as f32;
        draw(
            &mut renderer,
            &small_group(40.0 + 3.0 * step, 40.0 + 2.0 * step),
            &DamageSet::full(),
        );
    }
    assert_eq!(
        renderer.groups().allocations(),
        allocations,
        "no frame after the first allocated a target"
    );
}

#[test]
fn a_target_no_lease_uses_is_freed_after_its_idle_frames() {
    let Some((_guard, mut renderer, _)) = pair() else {
        return;
    };
    draw(&mut renderer, &small_group(40.0, 40.0), &DamageSet::full());
    assert!(renderer.groups().bytes() > 0);

    let mut plain = Scene::new();
    plain.begin_frame(Size::new(EXTENT, EXTENT));
    quad(
        &mut plain,
        (0.0, 0.0, EXTENT as f32, EXTENT as f32),
        [255; 3],
    );
    plain.finish(&DamageSet::full());
    for _ in 0..GroupPool::IDLE_FRAMES {
        draw(&mut renderer, &plain, &DamageSet::full());
    }
    assert!(
        renderer.groups().bytes() > 0,
        "a target stays for its idle frames"
    );
    draw(&mut renderer, &plain, &DamageSet::full());
    assert_eq!(renderer.groups().bytes(), 0, "and is freed after them");
}

/// A blurred group and a backdrop flush with the far edges of the surface, over a surface with a
/// coloured band along those edges.
fn at_the_far_edges() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(EXTENT, EXTENT));
    quad(
        &mut scene,
        (0.0, 0.0, EXTENT as f32, EXTENT as f32),
        [255; 3],
    );
    quad(&mut scene, (480.0, 0.0, 32.0, EXTENT as f32), [20, 40, 220]);
    quad(&mut scene, (0.0, 480.0, EXTENT as f32, 32.0), [220, 40, 20]);

    // As wide as the surface, so a target over the whole group is exactly one size class wide.
    let edge = (0.0, 400.0, EXTENT as f32, 112.0);
    let blurred = group(edge, 1.0, vec![Filter::Blur(6.0)]);
    scene.push_group(blurred.clone());
    quad(&mut scene, edge, [30, 160, 60]);
    scene.push_group(blurred.end());

    scene.push_backdrop(BackdropFilter::new(
        rect(440.0, 200.0, 72.0, 312.0),
        [Filter::Blur(8.0)].into_iter().collect(),
    ));

    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn a_partial_redraw_at_the_far_edges_draws_the_pixels_a_whole_one_does() {
    let Some((_guard, mut whole, mut partial)) = pair() else {
        return;
    };
    let reference = draw(&mut whole, &at_the_far_edges(), &DamageSet::full());
    let rects = [area(480, 440, 32, 40), area(200, 480, 40, 32)];
    let mut damaged = DamageSet::new();
    for rect in rects {
        damaged.absorb(rect);
    }

    // First after a frame of something else, then after a whole frame of the same scene, which
    // leaves larger targets for the partial frame to reuse.
    let mut other = Scene::new();
    other.begin_frame(Size::new(EXTENT, EXTENT));
    quad(
        &mut other,
        (0.0, 0.0, EXTENT as f32, EXTENT as f32),
        [10, 10, 90],
    );
    other.finish(&DamageSet::full());
    draw(&mut partial, &other, &DamageSet::full());
    for _ in 0..2 {
        let pixels = draw(&mut partial, &at_the_far_edges(), &damaged);
        for rect in rects {
            assert_eq!(
                difference_in(&reference, &pixels, rect),
                0,
                "a partial redraw differs from a whole one inside {rect:?}"
            );
        }
        draw(&mut partial, &at_the_far_edges(), &DamageSet::full());
    }
}
