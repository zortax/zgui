//! How many scratch layers a frame costs, and what happens when it wants more than there are.

mod support;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_render::{Renderer, VectorError, VectorFrame, VectorRaster};
use zgui_render_vector_vello::VelloRaster;
use zgui_render_vector_vello::raster::scratch::Scratch;
use zgui_scene::{ClipId, Scene};

use support::{Harness, Which, harness_at, opaque, path, present, quad, rect, scene_at, vector};

/// The extent every scene here fits in.
const SIDE: i32 = 640;

/// The side of one square in the grid scenes below.
const SQUARE: f32 = 8.0;

/// How far apart the grid scenes put their squares.
const PITCH: f32 = 16.0;

/// How many squares fit across the grid.
const COLUMNS: u32 = 39;

/// Where square `index` of a grid scene is drawn.
fn cell(index: u32) -> (f32, f32) {
    (
        PITCH + (index % COLUMNS) as f32 * PITCH,
        PITCH + (index / COLUMNS) as f32 * PITCH,
    )
}

/// The middle of the square `index` draws, which is where the picture is read.
fn middle_of(index: u32) -> (i32, i32) {
    let (x, y) = cell(index);
    (x as i32 + 4, y as i32 + 4)
}

/// A scene of `count` squares that nowhere touch, each with something drawn over it so each needs a
/// pass of its own.
///
/// The quad over each square is what splits the passes: it is an intervening primitive that covers
/// an item already accumulated, so the next square cannot join the same pass however far away it is.
fn one_pass_each(count: u32) -> Scene {
    let mut scene = scene_at(SIDE);
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    for index in 0..count {
        let (x, y) = cell(index);
        vector(
            &mut scene,
            index,
            path(rect(x, y, SQUARE, SQUARE)),
            opaque(255, 255, 255),
            ClipId::ROOT,
        );
        quad(
            &mut scene,
            rect(x, y, SQUARE, SQUARE),
            Color::srgb_u8(0, 0, 0, 0),
        );
    }
    scene.finish(&DamageSet::full());
    scene
}

/// A scene of `count` squares stacked exactly on top of each other, so no two of their passes can
/// share a layer.
///
/// Every square but the last is white and the last is red, which is what makes "the frame drew the
/// passes that fit and nothing else" a pixel somebody can read.
fn one_stack(count: u32) -> Scene {
    let mut scene = scene_at(SIDE);
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    for index in 0..count {
        let colour = if index + 1 == count {
            opaque(255, 0, 0)
        } else {
            opaque(255, 255, 255)
        };
        vector(
            &mut scene,
            index,
            path(rect(16.0, 16.0, 24.0, 24.0)),
            colour,
            ClipId::ROOT,
        );
        quad(
            &mut scene,
            rect(16.0, 16.0, 24.0, 24.0),
            Color::srgb_u8(0, 0, 0, 0),
        );
    }
    scene.finish(&DamageSet::full());
    scene
}

/// Rasterises `scene` through a rasteriser of its own on the harness's device, and answers what it
/// reported and how many layers its passes needed.
///
/// A rasteriser of its own rather than the harness's, because the harness's is behind the renderer
/// and what is wanted here is the layering, which a picture cannot show. The device is the
/// harness's: opening a second one would wait on the lock the harness is holding.
fn rasterise(harness: &Harness, scene: &Scene, side: i32) -> (Result<(), VectorError>, u32) {
    let gpu = std::sync::Arc::clone(harness.gpu());
    let mut raster = VelloRaster::new(&gpu, side as u32, side as u32).expect("a rasteriser");
    let plan = raster.plan(scene.pass_plan());
    raster.clear_targets(&plan);
    let placements = zgui_scene::Placements::of(&scene.spatial);
    let mut frame = VectorFrame::new(
        &plan,
        &scene.primitives.vectors,
        &scene.clips,
        &scene.paints,
        &placements,
    );
    let outcome = raster.prepare(&mut frame);
    (outcome, raster.depth())
}

/// Two passes that do not meet on the surface go into one layer, and each still composites where it
/// belongs.
///
/// This is what makes the scratch a function of the surface rather than of the busiest frame of the
/// session: a layer is in the surface's own coordinates, so passes that do not overlap there do not
/// overlap in it, and one layer holds as many of them as the frame has.
#[test]
fn two_disjoint_passes_share_a_layer_and_composite_in_order() {
    let Some(mut harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    let scene = one_pass_each(2);
    assert_eq!(
        scene.pass_plan().len(),
        2,
        "the fixture has to plan two passes, or nothing here is exercised"
    );
    let pixels = present(&mut harness.renderer, &scene);
    for index in 0..2 {
        let (x, y) = middle_of(index);
        assert_eq!(
            pixels.rgba(x, y),
            [255, 255, 255, 255],
            "square {index} was not composited"
        );
    }
    let (_, depth) = rasterise(&harness, &scene, SIDE);
    assert_eq!(
        depth, 1,
        "two passes that cannot touch each other must not cost two layers"
    );
}

/// Two passes that do meet go into layers of their own, whatever it costs.
///
/// The whole of the soundness argument: a layer holds its pass's coverage until that pass's
/// composite has read it, and every pass of a frame is rasterised before any of them is composited.
/// Two overlapping passes on one layer would have the second overwrite the first, and the first
/// composite would draw the second's paths — wrong content in the right place.
#[test]
fn two_overlapping_passes_do_not_share_a_layer() {
    let Some(mut harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    let scene = one_stack(2);
    assert_eq!(scene.pass_plan().len(), 2, "the fixture has to plan two");
    let pixels = present(&mut harness.renderer, &scene);
    assert_eq!(
        pixels.rgba(24, 24),
        [255, 0, 0, 255],
        "the upper of two stacked passes has to be the one on top"
    );
    let (_, depth) = rasterise(&harness, &scene, SIDE);
    assert_eq!(
        depth, 2,
        "two passes over the same texels shared a layer, so one composite drew the other's paths"
    );
}

/// A frame of five hundred passes draws all five hundred of them, in one layer.
///
/// Five hundred is the shape of a real document — a page of icons, borders and rules, each with
/// something painted over it — and it is past every ceiling this rasteriser ever had. What decides
/// the cost is how much of the frame overlaps, not how much of it there is.
#[test]
fn a_frame_planning_five_hundred_passes_draws_all_of_them() {
    let Some(mut harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    let count = 500;
    let scene = one_pass_each(count);
    assert_eq!(
        scene.pass_plan().len() as u32,
        count,
        "the fixture has to plan five hundred passes, or nothing here is exercised"
    );
    let dropped_before = counter::get(Counter::VectorFramesDropped);
    let pixels = present(&mut harness.renderer, &scene);
    for index in 0..count {
        let (x, y) = middle_of(index);
        assert_eq!(
            pixels.rgba(x, y),
            [255, 255, 255, 255],
            "square {index} of {count} is missing"
        );
    }
    let (outcome, depth) = rasterise(&harness, &scene, SIDE);
    outcome.expect("five hundred passes that nowhere touch each other all fit");
    assert_eq!(
        depth, 1,
        "five hundred passes that nowhere touch each other cost one layer between them"
    );
    if COUNTERS_ENABLED {
        assert_eq!(
            counter::get(Counter::VectorFramesDropped),
            dropped_before,
            "nothing was dropped, so nothing may be counted as dropped"
        );
    }
}

/// A frame with more mutually overlapping passes than layers says so, rather than sharing one.
///
/// The ceiling is still there and it is still a ceiling rather than a wrap. What changed is what
/// reaches it: not a pass count, but a stack of passes over one point deeper than the scratch.
#[test]
fn a_frame_past_the_layer_ceiling_reports_it_instead_of_sharing_a_layer() {
    let over = Scratch::MAX_LAYERS + 1;
    let scene = one_stack(over);
    assert_eq!(
        scene.pass_plan().len() as u32,
        over,
        "the fixture has to plan more overlapping passes than there are layers"
    );
    let Some(harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    let (outcome, _) = rasterise(&harness, &scene, SIDE);
    let failure = outcome.expect_err("more overlapping passes than layers cannot be rasterised");
    let VectorError::OutOfCapacity { prepared, .. } = failure else {
        panic!("the shortfall was reported as {failure:?} rather than as a capacity one");
    };
    assert_eq!(
        prepared as u32,
        Scratch::MAX_LAYERS,
        "every layer there is has to be used before a frame is told it ran out"
    );

    // One under the ceiling is the same fixture and does succeed, so the failure above is about the
    // ceiling and not about the shape of the scene.
    let under = one_stack(Scratch::MAX_LAYERS);
    let (outcome, _) = rasterise(&harness, &under, SIDE);
    outcome.expect("at the ceiling it fits");
}

/// Through a whole frame, the shortfall costs the passes that did not fit and nothing else.
///
/// A pass that was rasterised has a layer no overlapping pass shares and composites correctly, so
/// refusing the whole frame's vector content because one pass past the ceiling had nowhere to go
/// throws away sixty-four drawings to avoid one. What is not negotiable is the pass that did *not*
/// fit: it must draw nothing rather than whatever its layer happens to hold, and the frame must be
/// counted, because content the user asked for and did not get that nothing counts is a defect
/// nobody can find.
#[test]
fn a_frame_over_the_layer_ceiling_draws_what_fits() {
    let Some(mut harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    // The ceiling is reached rather than chosen, so a build with debug assertions on stops for it.
    // This frame goes over it deliberately.
    harness.renderer.set_vector_shortfall_fatal(false);

    // A frame that fits first, so the layers hold something recognisable that must not be replayed
    // by the composites of the frame that does not fit.
    let fits = one_pass_each(8);
    let drawn = present(&mut harness.renderer, &fits);
    let (x, y) = middle_of(0);
    assert_eq!(drawn.rgba(x, y), [255, 255, 255, 255], "the first frame");

    let dropped_before = counter::get(Counter::VectorFramesDropped);
    let over = one_stack(Scratch::MAX_LAYERS + 1);
    let outcome = harness.renderer.draw(&over, &DamageSet::full());
    assert!(
        outcome.stats().is_some(),
        "the frame still reaches the target: only the passes past the ceiling are lost"
    );
    let pixels = harness
        .renderer
        .read_presented()
        .expect("a stand-in surface can be read back");

    // The solid content, which never depended on a scratch layer at all.
    assert_eq!(
        pixels.rgba(SIDE - 4, SIDE - 4),
        [0, 0, 0, 255],
        "the frame's solid content is drawn whatever the rasteriser could not do"
    );
    // The stack is white up to the ceiling and red above it, so the pass that had nowhere to go is
    // the one that must not appear.
    assert_eq!(
        pixels.rgba(24, 24),
        [255, 255, 255, 255],
        "every pass the scratch had room for is composited, and the one past it draws nothing"
    );
    if !COUNTERS_ENABLED {
        return;
    }
    assert_eq!(
        counter::get(Counter::VectorFramesDropped) - dropped_before,
        1,
        "content the user asked for and did not get is counted exactly once"
    );
}
