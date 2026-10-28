//! Isolated groups: what a target of their own buys, and what it costs.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{Point, Rect, Size};
use zgui_render_wgpu::{GroupPool, TargetScale, wgpu};
use zgui_scene::{Filter, GroupBoundary, Quad, Scene};

use support::{SIDE, opaque, plain_renderer, present, rect};

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

/// Wraps `content` in a group of `opacity` carrying `filters`.
fn grouped(
    scene: &mut Scene,
    bounds: (f32, f32, f32, f32),
    opacity: f32,
    filters: &[Filter],
    content: impl FnOnce(&mut Scene),
) {
    let boundary = GroupBoundary::start(
        rect(bounds.0, bounds.1, bounds.2, bounds.3),
        opacity,
        zgui_scene::peniko::BlendMode::default(),
        filters.iter().copied().collect(),
    );
    scene.push_group(boundary.clone());
    content(scene);
    scene.push_group(boundary.end());
}

#[test]
fn two_overlapping_children_under_a_half_transparent_group_do_not_darken_twice() {
    // This is the whole reason a group has a target of its own. Folding the opacity into each
    // child's own alpha blends the overlap twice, so the shared region comes out darker than
    // either child; drawing the pair opaquely into a target and compositing it once does not.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    grouped(&mut scene, (16.0, 16.0, 96.0, 96.0), 0.5, &[], |scene| {
        quad(scene, (16.0, 16.0, 64.0, 64.0), [0, 0, 0]);
        quad(scene, (48.0, 48.0, 64.0, 64.0), [0, 0, 0]);
    });
    scene.finish(&DamageSet::full());

    let pixels = present(&mut renderer, &scene);
    let single = pixels.rgba(24, 24)[0];
    let overlap = pixels.rgba(64, 64)[0];
    assert!(
        (single as i32 - 127).abs() <= 2,
        "half of black over white is mid grey, and this read {single}"
    );
    assert_eq!(
        overlap, single,
        "the overlap composited once, so it is exactly as dark as the rest"
    );
}

#[test]
fn a_group_nested_four_deep_composites_every_level() {
    // A fixed depth limit is a correctness cliff, so there is none. Four half-transparent levels
    // multiply to one sixteenth, and a level that failed to composite would show as a step in that
    // product rather than as an error anywhere.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    grouped(&mut scene, (8.0, 8.0, 112.0, 112.0), 0.5, &[], |scene| {
        grouped(scene, (8.0, 8.0, 112.0, 112.0), 0.5, &[], |scene| {
            grouped(scene, (8.0, 8.0, 112.0, 112.0), 0.5, &[], |scene| {
                grouped(scene, (8.0, 8.0, 112.0, 112.0), 0.5, &[], |scene| {
                    quad(scene, (16.0, 16.0, 96.0, 96.0), [0, 0, 0]);
                });
            });
        });
    });
    scene.finish(&DamageSet::full());

    let pixels = present(&mut renderer, &scene);
    // Black at one sixteenth over white: 255 * 15/16 = 239.
    let inside = pixels.rgba(64, 64)[0];
    assert!(
        (inside as i32 - 239).abs() <= 2,
        "four halvings give 15/16 of white, and this read {inside}"
    );
    assert!(
        renderer.groups().peak() >= 4,
        "four levels were open at once, and the pool lent {}",
        renderer.groups().peak()
    );
}

#[test]
fn a_per_pixel_filter_costs_no_target_beyond_the_group_itself() {
    // A chain of per-pixel functions folds into one map that the composite applies as it samples,
    // so a group with five filters lends exactly the one target the group needed anyway.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    grouped(
        &mut scene,
        (16.0, 16.0, 96.0, 96.0),
        1.0,
        &[
            Filter::Saturate(0.0),
            Filter::Brightness(1.0),
            Filter::Invert(1.0),
        ],
        |scene| quad(scene, (16.0, 16.0, 96.0, 96.0), [0, 0, 0]),
    );
    scene.finish(&DamageSet::full());

    let pixels = present(&mut renderer, &scene);
    let inside = pixels.rgba(64, 64);
    assert_eq!(
        [inside[0], inside[1], inside[2]],
        [255, 255, 255],
        "grey then inverted is white"
    );
    assert_eq!(
        renderer.groups().peak(),
        1,
        "the filters travelled with the composite rather than in targets of their own"
    );
}

#[test]
fn the_pool_reuses_a_target_a_finished_group_returned() {
    // Two groups in sequence are two leases of one target, not two targets. Without the reuse a
    // list of fifty rows with an opacity transition would allocate fifty.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    for row in 0..8 {
        let top = row as f32 * 16.0;
        grouped(&mut scene, (0.0, top, 128.0, 16.0), 0.5, &[], |scene| {
            quad(scene, (0.0, top, 128.0, 16.0), [0, 0, 0]);
        });
    }
    scene.finish(&DamageSet::full());

    present(&mut renderer, &scene);
    assert_eq!(
        renderer.groups().peak(),
        1,
        "eight groups in sequence never held two targets at once"
    );
}

#[test]
fn a_group_the_pool_cannot_lend_a_target_for_draws_flat_rather_than_not_at_all() {
    // A budget that will not take even a half-resolution target is the one case where isolation
    // cannot happen. The content is then drawn straight into what is beneath it: the same picture
    // wherever the group's own effect is the identity, and a visibly flatter one where it is not.
    // What must not happen is the content disappearing, or the frame failing.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    renderer.set_group_budget(0);

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    grouped(&mut scene, (32.0, 32.0, 64.0, 64.0), 1.0, &[], |scene| {
        quad(scene, (32.0, 32.0, 64.0, 64.0), [0, 0, 0]);
    });
    scene.finish(&DamageSet::full());

    let pixels = present(&mut renderer, &scene);
    assert_eq!(renderer.groups().lent(), 0, "nothing was lent");
    assert_eq!(
        [
            pixels.rgba(64, 64)[0],
            pixels.rgba(64, 64)[1],
            pixels.rgba(64, 64)[2]
        ],
        [0, 0, 0],
        "the content is still drawn"
    );
    assert_eq!(pixels.rgba(16, 16), [255, 255, 255, 255]);
}

#[test]
fn per_channel_coverage_has_no_pipeline_for_an_isolated_target() {
    // Per-channel coverage writes no alpha, because dual-source blending consumes the coverage as
    // its blend factor; against a destination that is not opaque that is meaningless, and an
    // isolated target never is. Text landing in one is emitted as single-channel coverage instead,
    // so the variant is unreachable as well as wrong — and refusing to build it is what makes that
    // a fact about the renderer rather than a rule someone upstream has to remember.
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let gpu = std::sync::Arc::clone(renderer.gpu());
    let mut pipelines = zgui_render_wgpu::pipeline::Pipelines::new(&gpu);
    assert!(
        pipelines
            .get(
                &gpu,
                zgui_render_wgpu::pipeline::kind::PipelineKind::SubpixelSprite,
                GroupPool::FORMAT,
            )
            .is_none(),
        "an isolated target must have no per-channel coverage pipeline"
    );
    assert!(
        pipelines
            .get(
                &gpu,
                zgui_render_wgpu::pipeline::kind::PipelineKind::Composite,
                GroupPool::FORMAT,
            )
            .is_some(),
        "everything else exists for both attachment formats"
    );
}

#[test]
fn a_half_resolution_target_is_composited_in_the_right_place() {
    // The pool degrades to half resolution rather than to no isolation when its budget is full, so
    // a group composited from one has to land exactly where a full-resolution one would. Forcing
    // the budget down to a single half-resolution target is what makes that path taken rather than
    // written.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    grouped(&mut scene, (32.0, 32.0, 64.0, 64.0), 1.0, &[], |scene| {
        quad(scene, (32.0, 32.0, 64.0, 64.0), [0, 0, 0]);
    });
    scene.finish(&DamageSet::full());

    let full = present(&mut renderer, &scene).rgba(64, 64);
    assert_eq!([full[0], full[1], full[2]], [0, 0, 0]);

    // One half-resolution target's worth of budget, and not one texel more.
    let allocated: Size<i32, zgui_geom::Device> = Size::new(256, 256);
    let one_half_res = u64::from(GroupPool::FORMAT.block_copy_size(None).unwrap_or(8))
        * TargetScale::Half.extent(allocated).width as u64
        * TargetScale::Half.extent(allocated).height as u64;
    renderer.set_group_budget(one_half_res);

    let reduced = present(&mut renderer, &scene);
    assert_eq!(
        renderer.groups().degraded(),
        1,
        "the budget forced exactly one reduction"
    );
    assert_eq!(
        [
            reduced.rgba(64, 64)[0],
            reduced.rgba(64, 64)[1],
            reduced.rgba(64, 64)[2]
        ],
        [0, 0, 0],
        "the middle of the group is still the group's own colour"
    );
    assert_eq!(
        reduced.rgba(16, 16),
        [255, 255, 255, 255],
        "and nothing leaked outside it"
    );
    let _ = wgpu::TextureFormat::Rgba16Float;
}

#[test]
fn an_empty_group_composites_nothing_rather_than_what_the_pool_last_held() {
    // A group's target is lent by the pool and cleared by the first pass that *writes* into it. A
    // group with nothing in it has no such pass, so the clear never happens — and compositing the
    // target then reads whatever the previous lease left there and prints it inside the empty
    // element's own box, where it stays until something else draws over it.
    //
    // An element with `opacity` and no content is all it takes: a label with no text still has a
    // box, and a column of them down the side of a list is a column of windows onto the last thing
    // that used the pool.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    // Something distinctive, drawn through a group and so left in the pool's target.
    grouped(&mut scene, (64.0, 64.0, 64.0, 64.0), 1.0, &[], |scene| {
        quad(scene, (64.0, 64.0, 64.0, 64.0), [255, 0, 0]);
    });
    // Painted over, so the only red left anywhere is the red in the returned target.
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    // And an empty group over the same place, which leases that same target back.
    grouped(&mut scene, (64.0, 64.0, 64.0, 64.0), 1.0, &[], |_| {});
    scene.finish(&DamageSet::full());

    let pixels = present(&mut renderer, &scene);
    assert_eq!(
        pixels.rgba(96, 96),
        [255, 255, 255, 255],
        "the empty group put the last lease's content on the page"
    );
}

/// A group whose opacity and position moved twenty times composites exactly as a renderer that
/// only ever saw the last step does.
///
/// Every step is a whole frame: full damage, a fresh target lease, one composite. Nothing about
/// the pool, the plan or the composite may carry the earlier steps into the last one — a target
/// lent at a lower resolution because an earlier frame filled the budget, a sample taken through
/// a filtering sampler where none was needed — so the pixels of the two are compared exactly.
#[test]
fn a_group_animated_over_twenty_frames_composites_as_a_fresh_renderer_does() {
    let Some(mut animated) = plain_renderer() else {
        return;
    };
    let frame = |scene: &mut Scene, phase: f32| {
        scene.begin_frame(Size::new(SIDE, SIDE));
        quad(scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
        let shift = -20.0 * phase;
        grouped(
            scene,
            (40.0 + shift, 30.0, 41.0, 18.0),
            0.4 + 0.6 * phase,
            &[],
            |scene| {
                quad(scene, (40.0 + shift, 30.0, 41.0, 18.0), [40, 120, 200]);
                quad(scene, (48.0 + shift, 36.0, 12.0, 6.0), [255, 255, 255]);
            },
        );
        scene.finish(&DamageSet::full());
    };
    let mut scene = Scene::new();
    let mut last = None;
    for step in 0..=20 {
        frame(&mut scene, 0.9 * step as f32 / 20.0);
        last = Some(present(&mut animated, &scene));
    }
    let moved = last.expect("twenty-one frames were drawn");
    drop(animated);

    let Some(mut fresh) = plain_renderer() else {
        return;
    };
    let mut once = Scene::new();
    frame(&mut once, 0.9);
    let built = present(&mut fresh, &once);
    assert_eq!(
        moved.max_difference(&built),
        0,
        "the animated renderer's last frame differs from a fresh renderer's only frame"
    );
    assert_eq!(fresh.groups().degraded(), 0);
}

/// The same twenty steps, each drawn over the damage the step actually produced: where the
/// group was and where it is now, and nothing else.
///
/// This is the shape an animated badge has in a real frame. The composed target keeps every
/// pixel outside the damage, and the group's target is lent again from the pool with whatever
/// the last step left in it — so a composite that read outside the damage, or a target region
/// that was not cleared before it was drawn into, is what this finds.
#[test]
fn a_group_animated_under_partial_damage_composites_as_a_fresh_renderer_does() {
    let Some(mut animated) = plain_renderer() else {
        return;
    };
    let bounds = |phase: f32| (40.0 - 20.0 * phase, 30.0, 41.0, 18.0);
    let frame = |scene: &mut Scene, phase: f32, damage: &DamageSet| {
        scene.begin_frame(Size::new(SIDE, SIDE));
        quad(scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
        let at = bounds(phase);
        grouped(scene, at, 0.4 + 0.6 * phase, &[], |scene| {
            quad(scene, at, [40, 120, 200]);
            quad(scene, (at.0 + 8.0, at.1 + 6.0, 12.0, 6.0), [255, 255, 255]);
        });
        scene.finish(damage);
    };
    let damage_of = |from: f32, to: f32| {
        let mut damage = DamageSet::new();
        for phase in [from, to] {
            let (x, y, w, h) = bounds(phase);
            let corner: zgui_geom::Rect<i32, zgui_geom::Device> = zgui_geom::Rect::new(
                zgui_geom::Point::new(x.floor() as i32, y.floor() as i32),
                Size::new(w.ceil() as i32 + 1, h.ceil() as i32 + 1),
            );
            damage.absorb(corner);
        }
        damage
    };
    let mut scene = Scene::new();
    frame(&mut scene, 0.0, &DamageSet::full());
    let mut last = present(&mut animated, &scene);
    for step in 1..=20 {
        let (from, to) = (0.9 * (step - 1) as f32 / 20.0, 0.9 * step as f32 / 20.0);
        frame(&mut scene, to, &damage_of(from, to));
        last = present(&mut animated, &scene);
    }
    drop(animated);

    let Some(mut fresh) = plain_renderer() else {
        return;
    };
    let mut once = Scene::new();
    frame(&mut once, 0.9, &DamageSet::full());
    let built = present(&mut fresh, &once);
    assert_eq!(
        last.max_difference(&built),
        0,
        "the animated renderer's last frame differs from a fresh renderer's only frame"
    );
}

#[test]
fn a_half_resolution_group_ending_on_an_odd_pixel_keeps_its_last_row_and_column() {
    // A group from x = 32 to x = 95 ends half way through a half-resolution texel. That texel holds
    // the group's last column, and the pass that fills the target has to reach it.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    grouped(&mut scene, (32.0, 32.0, 63.0, 63.0), 1.0, &[], |scene| {
        quad(scene, (32.0, 32.0, 63.0, 63.0), [0, 0, 0]);
    });
    scene.finish(&DamageSet::full());

    let allocated: Size<i32, zgui_geom::Device> = Size::new(256, 256);
    let one_half_res = u64::from(GroupPool::FORMAT.block_copy_size(None).unwrap_or(8))
        * TargetScale::Half.extent(allocated).width as u64
        * TargetScale::Half.extent(allocated).height as u64;
    renderer.set_group_budget(one_half_res);
    let reduced = present(&mut renderer, &scene);
    assert_eq!(
        renderer.groups().degraded(),
        1,
        "the group is at half resolution"
    );

    let column = reduced.rgba(94, 64)[0];
    let row = reduced.rgba(64, 94)[0];
    assert!(
        column < 128 && row < 128,
        "the last column and row of a black group are mostly black: {column} and {row}"
    );
}

#[test]
fn a_pool_at_its_budget_degrades_what_is_lent_at_once_and_trims_at_the_end_of_the_frame() {
    // A target this frame lent and returned stays resident, because the plan still names it. The
    // budget limits what is lent at once, and the end of the frame frees targets until the pool is
    // within the budget again.
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let gpu = std::sync::Arc::clone(renderer.gpu());
    let region = Size::new(SIDE, SIDE);
    let allocated = GroupPool::new(region, 0).allocated_extent(TargetScale::Full);
    let one_full = u64::from(GroupPool::FORMAT.block_copy_size(None).unwrap_or(8))
        * allocated.width as u64
        * allocated.height as u64;
    let mut pool = GroupPool::new(region, one_full + one_full / 4);
    let other = wgpu::TextureFormat::Bgra8Unorm;

    let copy = pool
        .acquire(&gpu, TargetScale::Full, other)
        .expect("one copy target fits");
    pool.release(copy);
    let group = pool
        .acquire(&gpu, TargetScale::Full, GroupPool::FORMAT)
        .expect("one target lent at a time fits");
    assert_eq!(group.scale(), TargetScale::Full);
    assert_eq!(pool.degraded(), 0);
    assert!(
        pool.bytes() > pool.budget(),
        "this frame lent the copy target, and its plan still names it, so it stays resident"
    );
    let degraded = pool
        .acquire(&gpu, TargetScale::Full, GroupPool::FORMAT)
        .expect("a half-resolution target fits next to the lent one");
    assert_eq!(degraded.scale(), TargetScale::Half);
    assert_eq!(pool.degraded(), 1);
    assert!(
        pool.acquire_exact(&gpu, TargetScale::Full, GroupPool::FORMAT)
            .is_none(),
        "an exact lease never degrades, and the lent targets fill the budget"
    );

    pool.release_all();
    assert!(pool.bytes() <= pool.budget(), "the end of the frame trims");
    let full = pool
        .acquire(&gpu, TargetScale::Full, GroupPool::FORMAT)
        .expect("a full-resolution target fits in the next frame");
    assert_eq!(full.scale(), TargetScale::Full);
    assert_eq!(pool.degraded(), 1, "and this lease was not reduced");
    assert!(pool.bytes() <= pool.budget());
}

#[test]
fn leases_of_growing_regions_in_one_frame_neither_degrade_nor_lose_a_capture() {
    // Each lease is returned before the next is taken, so each fits the budget on its own. Each
    // is too large for the target before it, so the frame holds all of them.
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let gpu = std::sync::Arc::clone(renderer.gpu());
    let side = |length: i32| Rect::new(Point::new(0, 0), Size::new(length, length));
    let one = |length: u64| {
        u64::from(GroupPool::FORMAT.block_copy_size(None).unwrap_or(8)) * length * length
    };
    let mut pool = GroupPool::new(Size::new(1024, 1024), one(1024) * 3 / 2);
    for length in [250, 500, 750] {
        let slot = pool
            .acquire_covering(&gpu, TargetScale::Full, GroupPool::FORMAT, side(length))
            .expect("one lease at a time fits");
        pool.release(slot);
    }
    let capture = pool
        .acquire_exact_covering(&gpu, TargetScale::Full, GroupPool::FORMAT, side(1000))
        .expect("a capture is not lost to the targets returned earlier in the frame");
    assert_eq!(capture.scale(), TargetScale::Full);
    assert_eq!(pool.degraded(), 0);
    assert!(pool.bytes() > pool.budget());

    pool.release_all();
    assert!(pool.bytes() <= pool.budget(), "the end of the frame trims");
}
