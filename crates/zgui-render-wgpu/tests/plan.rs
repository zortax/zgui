//! The frame graph as a value: where a frame is cut, and why it is cut there.

mod support;

use std::path::Path;

use zgui_bits::DamageSet;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_render_wgpu::bind::globals::SubpixelOrder;
use zgui_render_wgpu::buffer::slots::SlotBuffer;
use zgui_render_wgpu::frame::build::{FramePlan, PlanBuilder};
use zgui_render_wgpu::frame::plan::plan_segments;
use zgui_render_wgpu::frame::segment::{PlannedDraw, Segment};
use zgui_render_wgpu::frame::target::TargetRef;
use zgui_render_wgpu::{GroupPool, wgpu};
use zgui_scene::{BackdropFilter, Filter, GroupBoundary, Quad, Scene};

use support::{SIDE, opaque, plain_renderer, rect};

/// The surface every plan here covers.
fn used() -> Rect<i32, Device> {
    Rect::new(Point::new(0, 0), Size::new(SIDE, SIDE))
}

/// Plans `scene` against `damage` on a real device, and returns the plan.
fn plan(gpu: &zgui_render_wgpu::Gpu, scene: &Scene, damage: &DamageSet) -> FramePlan {
    plan_with_vectors(gpu, scene, damage, None)
}

/// The same, with a resourced vector plan the composites can be planned from.
fn plan_with_vectors(
    gpu: &zgui_render_wgpu::Gpu,
    scene: &Scene,
    damage: &DamageSet,
    vectors: Option<&zgui_render::VectorPlan>,
) -> FramePlan {
    let mut pool = GroupPool::new(used().size, GroupPool::BUDGET);
    let mut globals = SlotBuffer::new::<zgui_render_wgpu::bind::globals::Globals>(gpu, "test.g");
    let mut blocks =
        SlotBuffer::new::<zgui_render_wgpu::pipeline::composite::CompositeParams>(gpu, "test.b");
    let mut instances = zgui_render_wgpu::buffer::vectors::VectorInstances::new(gpu);
    let builder = PlanBuilder::new(
        gpu,
        &mut pool,
        &mut globals,
        &mut blocks,
        &mut instances,
        SubpixelOrder::default(),
        zgui_scene::FrameClock::default(),
        &[],
        used().size,
        wgpu::TextureFormat::Bgra8Unorm,
        Size::new(256, 256),
    );
    plan_segments(builder, scene, damage, used(), &|_| None, vectors)
}

/// A scene of one quad, optionally wrapped in a group and optionally under a frosted panel.
fn scene_of(group: Option<Vec<Filter>>, backdrop: Option<f32>) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let paint = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(10, 20, 30)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        paint,
    ));
    match group {
        None => {
            scene.push_quad(Quad::filled(rect(16.0, 16.0, 64.0, 64.0), paint));
        }
        Some(filters) => {
            let boundary = GroupBoundary::start(
                rect(16.0, 16.0, 64.0, 64.0),
                0.5,
                zgui_scene::peniko::BlendMode::default(),
                filters.into_iter().collect(),
            );
            scene.push_group(boundary.clone());
            scene.push_quad(Quad::filled(rect(16.0, 16.0, 64.0, 64.0), paint));
            scene.push_group(boundary.end());
        }
    }
    if let Some(deviation) = backdrop {
        scene.push_backdrop(BackdropFilter::new(
            rect(32.0, 32.0, 48.0, 48.0),
            [Filter::Blur(deviation)].into_iter().collect(),
        ));
    }
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn every_damaged_rectangle_is_cleared_before_anything_is_drawn_into_it() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let scene = scene_of(None, None);

    let mut two = DamageSet::<4>::new();
    two.absorb(Rect::new(Point::new(0, 0), Size::new(32, 32)));
    two.absorb(Rect::new(Point::new(80, 80), Size::new(32, 32)));
    let planned = plan(renderer.gpu(), &scene, &two);

    let clears: Vec<&Segment> = planned
        .segments
        .iter()
        .filter(|segment| {
            segment
                .pass()
                .is_some_and(|pass| planned.draws_of(pass).first() == Some(&PlannedDraw::Clear))
        })
        .collect();
    assert_eq!(clears.len(), 2, "one clear per damaged rectangle");
    for segment in clears {
        let pass = segment.pass().expect("filtered to passes");
        assert_eq!(pass.target, TargetRef::Composed, "and only there");
        assert!(
            two.rects().contains(&pass.scissor),
            "the clear is scissored to the rectangle it is clearing: {:?}",
            pass.scissor
        );
    }
}

#[test]
fn a_group_is_three_passes_and_the_middle_one_writes_a_target_of_its_own() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let planned = plan(
        renderer.gpu(),
        &scene_of(Some(Vec::new()), None),
        &DamageSet::full(),
    );
    let targets: Vec<TargetRef> = planned.passes().map(|pass| pass.target).collect();
    assert_eq!(targets.len(), 3, "{targets:?}");
    assert_eq!(targets[0], TargetRef::Composed, "what is beneath the group");
    assert!(
        matches!(targets[1], TargetRef::Pool(_)),
        "the group's own target"
    );
    assert_eq!(targets[2], TargetRef::Composed, "and the composite back");

    let composites: usize = planned
        .draws
        .iter()
        .filter(|draw| matches!(draw, PlannedDraw::Composite { .. }))
        .count();
    assert_eq!(composites, 1, "a group composites exactly once");
}

#[test]
fn a_blur_is_three_filtering_passes_between_the_group_and_its_composite() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let planned = plan(
        renderer.gpu(),
        &scene_of(Some(vec![Filter::Blur(4.0)]), None),
        &DamageSet::full(),
    );
    let blurs: Vec<&PlannedDraw> = planned
        .draws
        .iter()
        .filter(|draw| matches!(draw, PlannedDraw::Blur { .. }))
        .collect();
    assert_eq!(
        blurs.len(),
        3,
        "a downsample and one pass along each axis: {blurs:?}"
    );
    assert!(
        matches!(
            blurs[0],
            PlannedDraw::Blur {
                downsample: true,
                ..
            }
        ),
        "the downsample comes first"
    );
    assert!(
        blurs[1..].iter().all(|draw| matches!(
            draw,
            PlannedDraw::Blur {
                downsample: false,
                ..
            }
        )),
        "and the two axis passes follow it"
    );
    for draw in &blurs {
        let PlannedDraw::Blur { source, .. } = draw else {
            unreachable!("filtered to blurs")
        };
        assert!(
            source.slot().is_some()
                || !matches!(
                    draw,
                    PlannedDraw::Blur {
                        downsample: false,
                        ..
                    }
                ),
            "an axis pass reads a target of the pool"
        );
    }
}

#[test]
fn a_backdrop_capture_is_the_one_thing_that_needs_the_encoder_and_no_pass_is_open_across_it() {
    // The whole reason a frame is planned before a pass is opened: a live pass holds the encoder
    // borrowed, so a copy between targets cannot happen while one is alive. Here that is a shape
    // the plan has rather than a comment — the capture is its own segment, and the segments either
    // side of it are passes that were closed to make room for it.
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let planned = plan(
        renderer.gpu(),
        &scene_of(None, Some(4.0)),
        &DamageSet::full(),
    );
    let captures: Vec<usize> = planned
        .segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| segment.encoder_op().is_some())
        .map(|(at, _)| at)
        .collect();
    assert_eq!(captures.len(), 1, "one backdrop, one capture");

    let at = captures[0];
    assert!(at > 0, "something was drawn before the capture read it");
    assert!(
        planned.segments[at - 1].pass().is_some(),
        "and the pass that drew it was closed to make room"
    );
    assert!(
        planned.segments[at + 1].pass().is_some(),
        "and the filtering that follows opens a pass of its own"
    );
}

#[test]
fn a_blend_mode_this_phase_cannot_composite_is_counted_rather_than_composited_wrongly() {
    // Every group here composites by plain source-over. A group asking for anything else — a
    // `mix-blend-mode`, or a Porter-Duff operator other than source-over — needs the destination
    // read, which a fragment shader cannot do to the attachment it is writing. Until that lands
    // the group is composited source-over and the shortfall is counted and logged, because a
    // silently wrong blend is indistinguishable from a correct one until somebody looks.
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let paint = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(10, 20, 30)));
    let boundary = GroupBoundary::start(
        rect(16.0, 16.0, 64.0, 64.0),
        1.0,
        zgui_scene::peniko::BlendMode::new(
            zgui_scene::peniko::Mix::Multiply,
            zgui_scene::peniko::Compose::SrcOver,
        ),
        Default::default(),
    );
    scene.push_group(boundary.clone());
    scene.push_quad(Quad::filled(rect(16.0, 16.0, 64.0, 64.0), paint));
    scene.push_group(boundary.end());
    scene.finish(&DamageSet::full());

    let planned = plan(renderer.gpu(), &scene, &DamageSet::full());
    assert_eq!(planned.unsupported_blends, 1);
    assert_eq!(
        plan(
            renderer.gpu(),
            &scene_of(Some(Vec::new()), None),
            &DamageSet::full()
        )
        .unsupported_blends,
        0,
        "and an ordinary group is not counted, so the number means something"
    );
}

#[test]
fn no_pass_outlives_its_encoder_by_being_told_to_forget_it() {
    // `forget_lifetime` would compile and would hide the ordering constraint the planner exists to
    // state. It is not used, and this is what keeps that true.
    let mut found: Vec<String> = Vec::new();
    walk(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
        &mut found,
    );
    assert!(
        found.is_empty(),
        "these discard the encoder's borrow instead of planning round it: {found:?}"
    );
}

/// Collects every source file under `directory` that discards a pass's borrow of its encoder.
fn walk(directory: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && std::fs::read_to_string(&path).is_ok_and(|text| text.contains("forget_lifetime"))
        {
            found.push(path.display().to_string());
        }
    }
}
