//! Does a path land in the frame at the right point in the order, with the right colour?

mod support;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{DevicePx, Point, Size, Vec2};
use zgui_render::Renderer;
use zgui_scene::{ClipId, GradientKind, Paint, PaintRef, Quad, VectorId, VectorItem};

use support::{
    Which, circle, counting_harness, harness, opaque, path, present, quad, rect, scene, vector,
};

/// Blue quad, red path over it, green quad over the path.
///
/// The whole interleave in one frame: the path has to occlude what was drawn before it and be
/// occluded by what was drawn after it, which is only true if the composite is inserted at the
/// path's own index in the batch stream rather than at the end of the frame.
#[test]
fn a_path_occludes_what_precedes_it_and_is_occluded_by_what_follows_it() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 255));
    vector(
        &mut scene,
        0,
        circle(64.0, 64.0, 40.0),
        opaque(255, 0, 0),
        ClipId::ROOT,
    );
    quad(&mut scene, rect(56.0, 56.0, 16.0, 16.0), opaque(0, 255, 0));
    scene.finish(&DamageSet::full());

    let pixels = present(&mut harness.renderer, &scene);
    assert_eq!(
        pixels.rgba(8, 8),
        [0, 0, 255, 255],
        "the rasterisation must not have wiped the frame"
    );
    assert_eq!(
        pixels.rgba(64, 30),
        [255, 0, 0, 255],
        "the path occludes the quad beneath it"
    );
    assert_eq!(
        pixels.rgba(64, 64),
        [0, 255, 0, 255],
        "the quad after it occludes the path"
    );
}

/// A half-transparent path over a gradient, against the arithmetic CSS specifies.
///
/// Two things at once: that the scratch holds *straight* alpha and the composite premultiplies, and
/// that the blend runs on gamma-encoded values. A blend in linear light would read about 225 where
/// this reads 191.
#[test]
fn a_half_transparent_path_over_a_gradient_blends_where_css_says_it_does() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    // A ramp from black to white across the frame, so every column beneath the path is different.
    let ramp = scene.paints.add(Paint::Gradient {
        kind: GradientKind::Linear {
            start: Point::new(DevicePx(0.0), DevicePx(0.0)),
            end: Point::new(DevicePx(128.0), DevicePx(0.0)),
        },
        stops: [
            zgui_color::GradientStop::new(0.0, opaque(0, 0, 0)),
            zgui_color::GradientStop::new(1.0, opaque(255, 255, 255)),
        ]
        .into_iter()
        .collect(),
        space: zgui_color::ColorSpace::Srgb,
        hue: zgui_color::HueInterpolation::Shorter,
        repeating: false,
    });
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 128.0, 128.0), ramp));

    let half_grey = scene.paints.solid(Color::srgb_u8(128, 128, 128, 128));
    scene.push_vector(VectorItem::filled(
        VectorId(0),
        path(rect(8.0, 32.0, 112.0, 64.0)),
        PaintRef::solid(half_grey),
    ));
    scene.finish(&DamageSet::full());

    let pixels = present(&mut harness.renderer, &scene);
    // The same column, above the path and inside it. What is above it is the ramp alone, so the
    // expected value is arithmetic on a measured background rather than on an assumed one.
    let alpha = 128.0 / 255.0;
    let mut discrimination = 0.0_f32;
    for x in [12, 48, 80, 116] {
        let background = f32::from(pixels.rgba(x, 16)[0]);
        let blended = pixels.rgba(x, 64);
        let expected = alpha * 128.0 + (1.0 - alpha) * background;
        for channel in 0..3 {
            assert!(
                (f32::from(blended[channel]) - expected).abs() <= 2.0,
                "at x = {x} over a background of {background}, the composite read \
                 {blended:?} where CSS says {expected}"
            );
        }
        // And the same blend in linear light would be visibly elsewhere, which is what makes the
        // check above a check rather than a restatement.
        let linear = |value: f32| {
            let value = value / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        let encode = |value: f32| {
            if value <= 0.003_130_8 {
                value * 12.92
            } else {
                1.055 * value.powf(1.0 / 2.4) - 0.055
            }
        };
        let in_linear = encode(alpha * linear(128.0) + (1.0 - alpha) * linear(background)) * 255.0;
        discrimination = discrimination.max((in_linear - expected).abs());
    }
    // The two answers coincide where the source and the background are close, so the check above
    // only means something because somewhere across the ramp they are far apart — and they are.
    assert!(
        discrimination > 8.0,
        "the ramp never separates the two blends by more than {discrimination}, so nothing above \
         distinguishes gamma-space compositing from linear-light compositing"
    );
}

/// The rasteriser's own clip, applied inside the scratch, against the same clip on a quad.
///
/// They will not be identical and are not asked to be: one edge is analytic area coverage and the
/// other is a distance field, and the two disagree over the edge band by design. What must hold is
/// that the interiors agree exactly and the disagreement stays inside a band one pixel wide.
#[test]
fn an_elliptical_residual_matches_the_quad_clip_inside_and_differs_only_at_the_edge() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let bounds = rect(24.0, 44.0, 80.0, 40.0);
    let radii = Vec2::new(DevicePx(32.0), DevicePx(12.0));

    // The clip as a quad clip: one item, so the chain is the pass's own and the composite binds it.
    let mut as_pass_clip = scene();
    quad(
        &mut as_pass_clip,
        rect(0.0, 0.0, 128.0, 128.0),
        opaque(0, 0, 0),
    );
    let clip = support::rounded(&mut as_pass_clip, ClipId::ROOT, bounds, radii);
    vector(
        &mut as_pass_clip,
        0,
        path(rect(0.0, 0.0, 128.0, 128.0)),
        opaque(255, 0, 0),
        clip,
    );
    as_pass_clip.finish(&DamageSet::full());
    let plan = as_pass_clip.pass_plan();
    assert_eq!(plan.clip_layers, 0, "one item's whole chain is the pass's");
    let quad_clipped = present(&mut harness.renderer, &as_pass_clip);

    // The same clip as a residual: a second item with a shallower chain drops the shared prefix to
    // nothing, so the rounded link has to be applied inside the scratch instead. The second item is
    // one fully transparent pixel in the corner, which contributes nothing to any pixel.
    let mut as_residual = scene();
    quad(
        &mut as_residual,
        rect(0.0, 0.0, 128.0, 128.0),
        opaque(0, 0, 0),
    );
    vector(
        &mut as_residual,
        1,
        path(rect(0.0, 0.0, 1.0, 1.0)),
        Color::srgb_u8(0, 0, 0, 0),
        ClipId::ROOT,
    );
    let clip = support::rounded(&mut as_residual, ClipId::ROOT, bounds, radii);
    vector(
        &mut as_residual,
        0,
        path(rect(0.0, 0.0, 128.0, 128.0)),
        opaque(255, 0, 0),
        clip,
    );
    as_residual.finish(&DamageSet::full());
    assert_eq!(
        as_residual.pass_plan().clip_layers,
        1,
        "the rounded link is absorbed into the rasteriser's own scene"
    );
    let residual_clipped = present(&mut harness.renderer, &as_residual);

    // The interior is exact, and so is everything well outside.
    for (x, y) in [(64, 64), (40, 64), (90, 64), (4, 4), (120, 120)] {
        assert_eq!(
            quad_clipped.rgba(x, y),
            residual_clipped.rgba(x, y),
            "the two mechanisms disagree at ({x}, {y}), which is not on any edge"
        );
    }

    // The disagreement exists, is confined to the edge, and is nowhere near total.
    let mut differing = 0;
    let mut worst = 0u8;
    for y in 0..128 {
        for x in 0..128 {
            let error = (0..4)
                .map(|c| quad_clipped.rgba(x, y)[c].abs_diff(residual_clipped.rgba(x, y)[c]))
                .max()
                .unwrap_or(0);
            if error > 0 {
                differing += 1;
            }
            worst = worst.max(error);
        }
    }
    println!("residual vs pass clip: worst {worst}, differing {differing}");
    assert!(
        worst > 0,
        "a test asserting the two agree exactly would be asserting something false; \
         analytic area coverage is not a distance field"
    );
    assert!(
        worst <= 96,
        "the edge band differs by {worst}, far more than antialiasing accounts for"
    );
    assert!(
        differing <= 600,
        "{differing} pixels differ, which is more than an edge band of this shape has"
    );
}

/// A frame with no paths does no rasterisation work at all.
///
/// Not "an empty pass": an empty pass over a whole surface still costs tens of microseconds of
/// processor time and several times that in latency, so the plan has to be empty and the rasteriser
/// has to be left alone entirely.
#[test]
fn a_frame_with_no_paths_runs_no_vector_work() {
    let Some((mut harness, work)) = counting_harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    for index in 0..61 {
        let offset = index as f32;
        quad(
            &mut scene,
            rect(offset, offset, 8.0, 8.0),
            opaque(200, 100, 50),
        );
    }
    scene.finish(&DamageSet::full());
    assert_eq!(scene.pass_plan().len(), 0);

    let outcome = harness.renderer.draw(&scene, &DamageSet::full());
    let stats = outcome.stats().expect("the frame reached the target");
    assert_eq!(stats.vector_passes, 0);

    let idle = *work.lock().unwrap_or_else(|held| held.into_inner());
    assert_eq!(idle.passes, 0, "no pass was planned");
    assert_eq!(
        idle.clears, 0,
        "nothing was cleared, because nothing was to be written"
    );
    assert_eq!(
        idle.preparations, 0,
        "the rasteriser was not entered at all, which is the point: an empty pass over a whole \
         surface is not free"
    );
    assert_eq!(
        idle.nanoseconds, 0,
        "zero time inside the rasteriser, and this counts every entry into it"
    );

    // The same frame with one path in it does enter the rasteriser, so the assertions above are
    // about this frame rather than about a rasteriser nothing ever calls.
    let mut with_path = scene;
    vector(
        &mut with_path,
        0,
        circle(64.0, 64.0, 20.0),
        opaque(255, 0, 0),
        ClipId::ROOT,
    );
    with_path.finish(&DamageSet::full());
    let outcome = harness.renderer.draw(&with_path, &DamageSet::full());
    assert_eq!(outcome.stats().expect("presented").vector_passes, 1);
    let busy = *work.lock().unwrap_or_else(|held| held.into_inner());
    assert_eq!(busy.preparations, 1);
    assert!(busy.nanoseconds > 0);
}

/// A scene whose only vector content misses the damage set costs no rasterisation either.
#[test]
fn an_item_outside_the_damage_set_costs_no_pass() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    vector(
        &mut scene,
        0,
        circle(96.0, 96.0, 16.0),
        opaque(255, 0, 0),
        ClipId::ROOT,
    );
    let mut damage = DamageSet::new();
    damage.absorb(zgui_geom::Rect::new(Point::new(0, 0), Size::new(32, 32)));
    scene.finish(&damage);

    assert_eq!(scene.pass_plan().len(), 0, "rule 1 dropped the only item");
    assert_eq!(scene.pass_plan().culled, 1);
    let outcome = harness.renderer.draw(&scene, &damage);
    assert_eq!(outcome.stats().expect("presented").vector_passes, 0);
}
