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
    let mut marks =
        SlotBuffer::new::<zgui_render_wgpu::pipeline::marks::MarkDraw>(gpu, "test.marks");
    let builder = PlanBuilder::new(
        gpu,
        &mut pool,
        &mut globals,
        &mut blocks,
        &mut instances,
        &mut marks,
        SubpixelOrder::default(),
        zgui_scene::FrameClock::default(),
        &[],
        used().size,
        wgpu::TextureFormat::Bgra8Unorm,
        Size::new(256, 256),
    );
    plan_segments(
        builder,
        scene,
        damage,
        scene.mark_plan(),
        used(),
        &|_| None,
        vectors,
    )
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

    let mut two: DamageSet = DamageSet::new();
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

/// A background, a rectangle in the top left, and a stack of lines and rectangles in the bottom
/// right.
///
/// The stack alternates two kinds, so each of its layers is a batch of its own, and none of those
/// batches reaches the top left.
fn corner_batches() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let paint = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(10, 20, 30)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        paint,
    ));
    scene.push_quad(Quad::filled(rect(8.0, 8.0, 24.0, 24.0), paint));
    for layer in 0..4 {
        let at = rect(80.0, 80.0 + 2.0 * layer as f32, 32.0, 8.0);
        if layer % 2 == 0 {
            scene.push_decoration(zgui_scene::Decoration::new(
                at,
                2.0,
                opaque(200, 0, 0),
                zgui_scene::prim::DecorationStyle::Solid,
            ));
        } else {
            scene.push_quad(Quad::filled(at, paint));
        }
    }
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn a_batch_that_misses_a_damaged_rectangle_is_not_drawn_into_it() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let scene = corner_batches();
    let batches: Vec<zgui_scene::Batch> = scene.batches().collect();
    assert_eq!(batches.len(), 5, "{batches:?}");

    let top_left = Rect::new(Point::new(0, 0), Size::new(40, 40));
    let bottom_right = Rect::new(Point::new(72, 72), Size::new(48, 48));
    let mut two = DamageSet::new();
    two.absorb(top_left);
    two.absorb(bottom_right);
    let planned = plan(renderer.gpu(), &scene, &two);

    let drawn = |scissor: Rect<i32, Device>| -> Vec<PlannedDraw> {
        let pass = planned
            .passes()
            .find(|pass| pass.scissor == scissor)
            .expect("each rectangle has a pass");
        planned.draws_of(pass).to_vec()
    };
    // The background reaches every pixel, and the stack only the bottom right.
    assert_eq!(
        drawn(top_left),
        vec![PlannedDraw::Clear, PlannedDraw::Batch(batches[0].clone())]
    );
    let everything: Vec<PlannedDraw> = core::iter::once(PlannedDraw::Clear)
        .chain(batches.iter().cloned().map(PlannedDraw::Batch))
        .collect();
    assert_eq!(drawn(bottom_right), everything);
    assert_eq!(planned.culled, 4, "the four layers of the stack, once");
}

#[test]
fn one_damaged_rectangle_draws_every_batch() {
    // One rectangle replays the stream once, and the display list already left out what misses it.
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let scene = corner_batches();
    let mut one = DamageSet::new();
    let top_left = Rect::new(Point::new(0, 0), Size::new(40, 40));
    one.absorb(top_left);
    let planned = plan(renderer.gpu(), &scene, &one);

    let pass = planned.passes().next().expect("the rectangle has a pass");
    assert_eq!(pass.scissor, top_left);
    assert_eq!(planned.draws_of(pass).len(), 1 + 5);
    assert_eq!(planned.culled, 0);
}

#[test]
fn inside_a_group_a_batch_is_culled_against_the_group_region() {
    // A blur reads past the damaged rectangle, so content beside it still shapes the pixels inside
    // it and is drawn into the group. A rectangle the group's ink misses lends no target at all.
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let paint = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(10, 20, 30)));
    let boundary = GroupBoundary::start(
        rect(0.0, 0.0, 64.0, 64.0),
        1.0,
        zgui_scene::peniko::BlendMode::default(),
        [Filter::Blur(3.0)].into_iter().collect(),
    );
    scene.push_group(boundary.clone());
    scene.push_quad(Quad::filled(rect(8.0, 8.0, 16.0, 16.0), paint));
    scene.push_decoration(zgui_scene::Decoration::new(
        rect(44.0, 8.0, 8.0, 8.0),
        2.0,
        opaque(200, 0, 0),
        zgui_scene::prim::DecorationStyle::Solid,
    ));
    scene.push_group(boundary.end());
    scene.finish(&DamageSet::full());

    let mut two = DamageSet::new();
    two.absorb(Rect::new(Point::new(0, 0), Size::new(40, 40)));
    two.absorb(Rect::new(Point::new(96, 96), Size::new(16, 16)));
    let planned = plan(renderer.gpu(), &scene, &two);

    let line = scene
        .batches()
        .find(|batch| matches!(batch, zgui_scene::Batch::Decorations(_)))
        .expect("the line is a batch of its own");
    let into_groups = planned
        .passes()
        .filter(|pass| matches!(pass.target, TargetRef::Pool(_)))
        .filter(|pass| {
            planned
                .draws_of(pass)
                .contains(&PlannedDraw::Batch(line.clone()))
        })
        .count();
    assert_eq!(
        into_groups, 1,
        "the line is drawn into the group in the rectangle its blur reaches"
    );
    assert!(
        planned
            .passes()
            .filter(|pass| matches!(pass.target, TargetRef::Pool(_)))
            .all(|pass| pass
                .scissor
                .intersects(Rect::new(Point::new(0, 0), Size::new(40, 40)))),
        "the rectangle the group misses draws into no target"
    );
}

#[test]
fn a_group_region_is_cut_to_the_damaged_rectangle_grown_by_what_its_filters_read() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let scene = |filters: Vec<Filter>| {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(SIDE, SIDE));
        let paint = scene
            .paints
            .add(zgui_scene::Paint::Solid(opaque(10, 20, 30)));
        let boundary = GroupBoundary::start(
            rect(0.0, 0.0, SIDE as f32, SIDE as f32),
            0.5,
            zgui_scene::peniko::BlendMode::default(),
            filters.into_iter().collect(),
        );
        scene.push_group(boundary.clone());
        scene.push_quad(Quad::filled(
            rect(0.0, 0.0, SIDE as f32, SIDE as f32),
            paint,
        ));
        scene.push_group(boundary.end());
        scene.finish(&DamageSet::full());
        scene
    };
    let damaged = Rect::new(Point::new(56, 56), Size::new(16, 16));
    let mut damage = DamageSet::new();
    damage.absorb(damaged);
    let region = |filters: Vec<Filter>| {
        let planned = plan(renderer.gpu(), &scene(filters), &damage);
        let pass = planned
            .passes()
            .find(|pass| matches!(pass.target, TargetRef::Pool(_)))
            .expect("the group draws into a target");
        let TargetRef::Pool(slot) = pass.target else {
            unreachable!()
        };
        assert!(
            Rect::new(slot.origin(), slot.covered()).contains_rect(pass.scissor),
            "the lease covers the region drawn into it"
        );
        pass.scissor
    };

    let plain = region(Vec::new());
    assert!(plain.contains_rect(damaged));
    assert!(
        plain.size.width < 32 && plain.size.height < 32,
        "an unfiltered group reads only a few pixels past the rectangle: {plain:?}"
    );
    let blurred = region(vec![Filter::Blur(4.0)]);
    assert!(
        blurred.contains_rect(Rect::new(Point::new(42, 42), Size::new(44, 44))),
        "a blurred group reads three deviations and the resampling reach past it: {blurred:?}"
    );
    assert!(blurred.size.width < SIDE && blurred.size.height < SIDE);
}

#[test]
fn a_group_region_grows_by_what_a_backdrop_in_it_reads() {
    // A backdrop reads the target of the group around it past what it writes, so that group's
    // region grows by the backdrop's reach, even where the rectangle misses what the backdrop reads.
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let scene = |backdrop: bool| {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(SIDE, SIDE));
        let paint = scene
            .paints
            .add(zgui_scene::Paint::Solid(opaque(10, 20, 30)));
        let boundary = GroupBoundary::start(
            rect(0.0, 0.0, SIDE as f32, SIDE as f32),
            0.5,
            zgui_scene::peniko::BlendMode::default(),
            Default::default(),
        );
        scene.push_group(boundary.clone());
        scene.push_quad(Quad::filled(
            rect(0.0, 0.0, SIDE as f32, SIDE as f32),
            paint,
        ));
        if backdrop {
            scene.push_backdrop(BackdropFilter::new(
                rect(80.0, 80.0, 32.0, 32.0),
                [Filter::Blur(8.0)].into_iter().collect(),
            ));
        }
        scene.push_group(boundary.end());
        scene.finish(&DamageSet::full());
        scene
    };
    let damaged = Rect::new(Point::new(16, 16), Size::new(8, 8));
    let mut damage = DamageSet::new();
    damage.absorb(damaged);
    let region = |backdrop: bool| {
        let planned = plan(renderer.gpu(), &scene(backdrop), &damage);
        planned
            .passes()
            .find(|pass| matches!(pass.target, TargetRef::Pool(_)))
            .expect("the group draws into a target")
            .scissor
    };
    let plain = region(false);
    let holding = region(true);
    let reach = 3 * 8 + 2;
    assert!(
        holding.contains_rect(Rect::new(
            Point::new(16, 16),
            Size::new(8 + reach, 8 + reach)
        )),
        "the region grows by the backdrop's reach: {holding:?} against {plain:?}"
    );
    assert!(holding.contains_rect(plain) && holding != plain);
}

/// A scene of three squares stacked over one point, each in a vector pass of its own.
fn three_stacked_passes(damage: &DamageSet) -> Scene {
    use zgui_scene::kurbo::Shape as _;

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let paint = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(10, 20, 30)));
    let clear = scene
        .paints
        .add(zgui_scene::Paint::Solid(zgui_color::Color::srgb_u8(
            0, 0, 0, 0,
        )));
    for index in 0..3 {
        let square = std::sync::Arc::new(
            zgui_scene::kurbo::Rect::new(16.0, 16.0, 112.0, 112.0).to_path(0.1),
        );
        scene.push_vector(zgui_scene::VectorItem::filled(
            zgui_scene::VectorId(index),
            square,
            paint,
        ));
        scene.push_quad(Quad::filled(rect(16.0, 16.0, 96.0, 96.0), clear));
    }
    scene.finish(damage);
    scene
}

/// A resourced plan of `scene`'s passes, rasterised in the given rounds.
fn in_rounds(scene: &Scene, rounds: &[usize]) -> zgui_render::VectorPlan {
    let planned = scene.pass_plan();
    assert_eq!(planned.passes.len(), rounds.len(), "one round per pass");
    let mut plan = zgui_render::VectorPlan::resourcing(planned);
    for (pass, &round) in planned.passes.iter().zip(rounds) {
        plan.passes.push(zgui_render::VectorPass {
            region: pass.region,
            raster_region: Rect::new(Point::new(0, 0), pass.region.size),
            target: zgui_render::VectorTarget(0),
            round,
            items: pass.items.clone(),
            clip: pass.clip,
            instanced: pass.instanced,
            clamped: pass.clamped,
        });
    }
    plan
}

/// The rounds a plan rasterises between its runs, in order.
fn rounds_of(planned: &FramePlan) -> Vec<usize> {
    planned.segments.iter().filter_map(Segment::round).collect()
}

#[test]
fn a_later_vector_round_cuts_the_plan_before_its_first_composite() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let scene = three_stacked_passes(&DamageSet::full());
    let vectors = in_rounds(&scene, &[0, 1, 1]);
    let planned = plan_with_vectors(renderer.gpu(), &scene, &DamageSet::full(), Some(&vectors));

    assert_eq!(
        rounds_of(&planned),
        [1],
        "round zero is rasterised before the frame, and round one once, before its first pass"
    );
    let runs: Vec<_> = planned.runs().collect();
    assert_eq!(runs.len(), 2, "one cut, two runs");
    assert_eq!(runs[0].0, None);
    assert_eq!(runs[1].0, Some(1));
    let composites = |segments: &[Segment]| {
        segments
            .iter()
            .filter_map(Segment::pass)
            .flat_map(|pass| planned.draws_of(pass))
            .filter(|draw| matches!(draw, PlannedDraw::Vector { .. }))
            .count()
    };
    assert_eq!(
        (composites(runs[0].1), composites(runs[1].1)),
        (1, 2),
        "each composite is recorded after the round that rasterised its pass"
    );
    let reopened = runs[1].1.iter().find_map(Segment::pass).expect("a pass");
    assert_eq!(
        reopened.target,
        TargetRef::Composed,
        "the cut pass opens again on the target it was drawing into"
    );
}

#[test]
fn each_damaged_rectangle_runs_the_vector_rounds_from_the_first() {
    let Some(renderer) = plain_renderer() else {
        return;
    };
    let mut two = DamageSet::new();
    two.absorb(Rect::new(Point::new(0, 0), Size::new(32, 32)));
    two.absorb(Rect::new(Point::new(80, 80), Size::new(32, 32)));
    let scene = three_stacked_passes(&two);
    let vectors = in_rounds(&scene, &[0, 1, 1]);
    let planned = plan_with_vectors(renderer.gpu(), &scene, &two, Some(&vectors));
    assert_eq!(
        rounds_of(&planned),
        [1, 0, 1],
        "the second rectangle needs round zero back before it can draw its first composite"
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
