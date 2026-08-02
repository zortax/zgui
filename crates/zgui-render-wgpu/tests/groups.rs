//! Isolated groups: what a target of their own buys, and what it costs.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::Size;
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
