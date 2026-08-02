//! How a rasterised pass gets back into the frame: one draw for the pass, or one per item.

mod support;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{DevicePx, Vec2};
use zgui_profile::{Counter, counter};
use zgui_scene::{ClipId, ClipLink, Scene};

use support::{
    SIDE, Which, circle, harness, harness_at, opaque, path, present, quad, rect, scene, scene_at,
    vector,
};

/// Four disjoint circles, laid out so no two of them touch.
fn four_circles(scene: &mut Scene) {
    for (index, (x, y)) in [(32.0, 32.0), (96.0, 32.0), (32.0, 96.0), (96.0, 96.0)]
        .into_iter()
        .enumerate()
    {
        vector(
            scene,
            index as u32,
            circle(x, y, 20.0),
            opaque(255, 64, 32),
            ClipId::ROOT,
        );
    }
}

/// A pass composited one item at a time is pixel-identical to the same items composited one pass at
/// a time.
///
/// The soundness condition is that no two items of the pass overlap each other; under it the union
/// of the per-item quads reads every painted part of the scratch exactly once, so the two have to
/// agree exactly. The comparison partner is built without any switch to flip: a fully transparent
/// quad drawn after each circle contributes nothing to any pixel and forces the coalescer to end
/// the pass, which is precisely *k* passes of one item each.
#[test]
fn an_instanced_composite_is_identical_to_k_separate_passes() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };

    let mut instanced = scene();
    quad(
        &mut instanced,
        rect(0.0, 0.0, 128.0, 128.0),
        opaque(0, 0, 32),
    );
    four_circles(&mut instanced);
    instanced.finish(&DamageSet::full());
    assert_eq!(instanced.pass_plan().len(), 1, "nothing intervenes");
    assert!(
        instanced.pass_plan().passes[0].instanced,
        "four disjoint items are compositable one at a time"
    );
    assert_eq!(instanced.pass_plan().items.len(), 4);
    let one_pass = present(&mut harness.renderer, &instanced);

    let mut separate = scene();
    quad(
        &mut separate,
        rect(0.0, 0.0, 128.0, 128.0),
        opaque(0, 0, 32),
    );
    for (index, (x, y)) in [(32.0, 32.0), (96.0, 32.0), (32.0, 96.0), (96.0, 96.0)]
        .into_iter()
        .enumerate()
    {
        vector(
            &mut separate,
            index as u32,
            circle(x, y, 20.0),
            opaque(255, 64, 32),
            ClipId::ROOT,
        );
        // Invisible, and therefore not part of what is being compared — but it is a non-vector
        // primitive drawn after that circle and over its ink, which is exactly what ends a pass.
        quad(
            &mut separate,
            rect(x - 20.0, y - 20.0, 40.0, 40.0),
            Color::srgb_u8(0, 0, 0, 0),
        );
    }
    separate.finish(&DamageSet::full());
    assert_eq!(
        separate.pass_plan().len(),
        4,
        "each circle has something drawn over it, so each needs a pass"
    );
    let four_passes = present(&mut harness.renderer, &separate);

    assert_eq!(
        one_pass.max_difference(&four_passes),
        0,
        "compositing a pass one item at a time changed a pixel"
    );
    // And the circles are actually there, so the comparison is between two pictures rather than
    // between two empty frames.
    assert_eq!(one_pass.rgba(32, 32), [255, 64, 32, 255]);
    assert_eq!(one_pass.rgba(96, 96), [255, 64, 32, 255]);
}

/// A pass whose items overlap is composited once, and the overlap is not blended twice.
///
/// This is the other half of the same soundness condition. Two half-transparent items over each
/// other must read as one source-over of the pair; reading the scratch twice would show as a
/// visibly darker patch exactly where they meet.
#[test]
fn overlapping_items_share_one_composite_and_their_overlap_is_blended_once() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    quad(
        &mut scene,
        rect(0.0, 0.0, 128.0, 128.0),
        opaque(255, 255, 255),
    );
    let translucent = Color::srgb_u8(0, 0, 0, 128);
    vector(
        &mut scene,
        0,
        path(rect(16.0, 48.0, 64.0, 32.0)),
        translucent,
        ClipId::ROOT,
    );
    vector(
        &mut scene,
        1,
        path(rect(48.0, 48.0, 64.0, 32.0)),
        translucent,
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    assert_eq!(scene.pass_plan().len(), 1);
    assert!(
        !scene.pass_plan().passes[0].instanced,
        "two items that overlap each other must not be composited one at a time"
    );

    let pixels = present(&mut harness.renderer, &scene);
    let single = pixels.rgba(24, 64)[0];
    let overlap = pixels.rgba(64, 64)[0];
    // Inside the scratch the two half-transparent fills already composite over each other, so the
    // overlap is one source-over of the *pair* — about a quarter of white, not the eighth a
    // twice-read scratch would give and not the half a single fill gives.
    assert!(
        single.abs_diff(127) <= 3,
        "one half-transparent fill over white read {single}"
    );
    assert!(
        overlap.abs_diff(63) <= 4,
        "the overlap read {overlap}, which is neither one source-over of the pair nor anything \
         a correct composite produces"
    );
}

/// Items disjoint only in fractions of a pixel are not composited one at a time.
///
/// The soundness condition is about the *quads* a per-item composite draws, and a quad covers whole
/// pixels. Two items whose ink is disjoint as floats — one ending at 40.3, the next starting at 40.5
/// — round to whole-pixel rectangles that share the column between them, and a per-item composite
/// would read that column out of the scratch and blend it twice. Half-transparent fills make the
/// difference visible: the shared column reads darker than either fill and darker than their sum.
#[test]
fn items_disjoint_only_between_pixel_centres_are_not_composited_one_at_a_time() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    quad(
        &mut scene,
        rect(0.0, 0.0, 128.0, 128.0),
        opaque(255, 255, 255),
    );
    let translucent = Color::srgb_u8(0, 0, 0, 128);
    vector(
        &mut scene,
        0,
        path(rect(16.0, 40.0, 24.3, 32.0)),
        translucent,
        ClipId::ROOT,
    );
    vector(
        &mut scene,
        1,
        path(rect(40.5, 40.0, 24.0, 32.0)),
        translucent,
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    let plan = scene.pass_plan();
    assert_eq!(plan.len(), 1, "nothing intervenes, so this is one pass");
    // The premise: as whole-pixel rectangles the two inks really do share a column, which is what
    // there is to get wrong. Asserting it here keeps the flag assertion below from passing because
    // the fixture stopped overlapping.
    let (left, right) = (plan.items[0].ink, plan.items[1].ink);
    assert!(
        left.right() > right.origin.x,
        "the two whole-pixel inks {left:?} and {right:?} do not share a column, so this fixture no \
         longer exercises anything"
    );
    assert!(
        !plan.passes[0].instanced,
        "a per-item composite would blend the shared column twice"
    );

    let pixels = present(&mut harness.renderer, &scene);
    // The shared column carries 0.3 of one fill and 0.5 of the other, so about 0.8 of a
    // half-transparent black over white: darker than either fill alone and lighter than the two
    // blended over each other, which is what reading the scratch twice would give.
    let shared = pixels.rgba(40, 56)[0];
    assert!(
        (140..=180).contains(&shared),
        "the shared column read {shared}, where one composite of it reads about 163 and a column \
         composited twice reads about 104"
    );
    assert_eq!(pixels.rgba(24, 56), [127, 127, 127, 255], "the first fill");
    assert_eq!(pixels.rgba(56, 56), [127, 127, 127, 255], "the second");
}

/// Twelve differently clipped icons cost one pass and twelve clip layers, and all twelve appear.
///
/// The failure this exists to catch is silent: a pass composited as one draw binds one clip, so if
/// each icon's own rounded clip were treated as the pass's, eleven of the twelve would be discarded
/// for lying outside it and the frame would look almost right.
#[test]
fn rounded_clipped_icons_cost_one_pass_and_every_one_of_them_appears() {
    let side = 512;
    let Some(mut harness) = harness_at(side, Which::Vello) else {
        return;
    };
    let mut scene = scene_at(side);
    quad(
        &mut scene,
        rect(0.0, 0.0, side as f32, side as f32),
        opaque(0, 0, 0),
    );
    let container = rect(8.0, 8.0, 496.0, 48.0);
    let row = scene.clips.only(ClipLink::rect(container));
    let mut centres = Vec::new();
    for index in 0..12u32 {
        let x = 16.0 + index as f32 * 40.0;
        let bounds = rect(x, 12.0, 40.0, 40.0);
        let clip = scene
            .clips
            .push(row, ClipLink::rounded(bounds, Vec2::splat(DevicePx(20.0))));
        vector(&mut scene, index, path(bounds), opaque(240, 240, 240), clip);
        centres.push((x as i32 + 20, 32));
    }
    // Both counters are the display list's, recorded when it is finished, so the reset happens
    // before that and not before the frame: a renderer that recorded them again would double them.
    counter::reset();
    scene.finish(&DamageSet::full());

    let plan = scene.pass_plan();
    assert_eq!(plan.len(), 1, "nothing is drawn between the icons");
    assert_eq!(
        plan.clip_layers, 12,
        "each icon's own clip is absorbed as a layer rather than paid for with a pass"
    );

    let pixels = present(&mut harness.renderer, &scene);
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(
            counter::get(Counter::VelloPasses),
            1,
            "one pass, counted once"
        );
        assert_eq!(counter::get(Counter::VectorClipLayers), 12);
    }

    for (index, (x, y)) in centres.iter().copied().enumerate() {
        let colour = pixels.rgba(x, y);
        assert_eq!(
            colour,
            [240, 240, 240, 255],
            "icon {index} at ({x}, {y}) did not appear"
        );
    }
    // And each icon's clip really did round it off, so the twelve are twelve circles rather than
    // one long bar the shared container clip would have produced.
    let between = pixels.rgba(16 + 40 - 1, 12);
    assert_eq!(
        between,
        [0, 0, 0, 255],
        "the corner between two icons is inside neither, so it must be the background"
    );
}

/// A pass's own clip is bound by the composite, so vector content is clipped like everything else.
#[test]
fn a_pass_clip_cuts_the_composite_exactly_where_a_quad_would_be_cut() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let bounds = rect(32.0, 32.0, 64.0, 64.0);

    let mut vectors = scene();
    quad(&mut vectors, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    let clip = vectors.clips.only(ClipLink::rect(bounds));
    vector(
        &mut vectors,
        0,
        path(rect(0.0, 0.0, 128.0, 128.0)),
        opaque(255, 0, 0),
        clip,
    );
    vectors.finish(&DamageSet::full());
    let with_vector = present(&mut harness.renderer, &vectors);

    let mut quads = scene();
    quad(&mut quads, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    let clip = quads.clips.only(ClipLink::rect(bounds));
    let fill = support::solid(&mut quads, opaque(255, 0, 0));
    quads.push_quad(zgui_scene::Quad::filled(rect(0.0, 0.0, 128.0, 128.0), fill).clipped(clip));
    quads.finish(&DamageSet::full());
    let with_quad = present(&mut harness.renderer, &quads);

    // A square-cornered clip is a hard edge in both pipelines — the same rectangle test, in the
    // same shared function — so this one really is exact.
    assert_eq!(
        with_vector.max_difference(&with_quad),
        0,
        "the same clip applied to a composite and to a quad disagreed"
    );
    assert_eq!(with_vector.rgba(SIDE / 2, SIDE / 2), [255, 0, 0, 255]);
    assert_eq!(with_vector.rgba(8, 8), [0, 0, 0, 255]);
}

/// Every pass of a frame gets a scratch layer of its own.
///
/// This is not a resourcing nicety. *Every* pass is rasterised before *any* of them is composited,
/// because the rasteriser submits work of its own and that has to precede the frame's own recording
/// — so two passes sharing a layer would have the second overwrite the first, and the first
/// composite would then draw the second's paths in the first's place.
#[test]
fn eight_passes_keep_eight_different_pictures() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let colours: [[u8; 3]; 8] = [
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 0],
        [255, 0, 255],
        [0, 255, 255],
        [255, 128, 0],
        [128, 0, 255],
    ];
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    let mut centres = Vec::new();
    for (index, colour) in colours.into_iter().enumerate() {
        let x = 8.0 + (index % 4) as f32 * 30.0;
        let y = 8.0 + (index / 4) as f32 * 60.0;
        vector(
            &mut scene,
            index as u32,
            path(rect(x, y, 24.0, 24.0)),
            opaque(colour[0], colour[1], colour[2]),
            ClipId::ROOT,
        );
        // Something drawn over each one, which is what forces a pass per region.
        quad(
            &mut scene,
            rect(x, y, 24.0, 24.0),
            Color::srgb_u8(0, 0, 0, 0),
        );
        centres.push((x as i32 + 12, y as i32 + 12, colour));
    }
    scene.finish(&DamageSet::full());
    assert_eq!(
        scene.pass_plan().len(),
        8,
        "eight regions with something over each is eight passes"
    );

    let pixels = present(&mut harness.renderer, &scene);
    for (index, (x, y, colour)) in centres.into_iter().enumerate() {
        assert_eq!(
            pixels.rgba(x, y),
            [colour[0], colour[1], colour[2], 255],
            "pass {index} composited something other than its own content"
        );
    }
}
