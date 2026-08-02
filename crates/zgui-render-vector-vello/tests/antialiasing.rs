//! Which coverage method the path renderer is configured for, decided by measurement.
//!
//! Analytic area coverage is faster and re-uploads no sample-mask table, but whether the conflation
//! artefacts it can produce are visible on real content is a property of the content and not of the
//! algorithm. So the two are compared on the shapes that provoke them — overlapping strokes, a
//! rounded icon, a self-intersecting path — before there is any recorded picture to re-baseline.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::{DevicePx, Vec2};
use zgui_render_vector_vello::VelloRaster;
use zgui_scene::{ClipId, ClipLink, PaintRef, Scene, VectorId, VectorItem};

use support::{Which, circle, harness, opaque, present, quad, rect, scene, solid, vector};

/// The three shapes that provoke conflation, in one scene.
fn provoking() -> Scene {
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));

    // Two strokes crossing each other, which is where a coverage-accumulating rasteriser can show a
    // seam along the shared edge.
    let stroke = solid(&mut scene, opaque(255, 255, 255));
    let mut diagonal = kurbo::BezPath::new();
    diagonal.move_to((8.0, 8.0));
    diagonal.line_to((56.0, 56.0));
    scene.push_vector(VectorItem::stroked(
        VectorId(0),
        Arc::new(diagonal),
        stroke,
        6.0,
    ));
    let mut opposite = kurbo::BezPath::new();
    opposite.move_to((56.0, 8.0));
    opposite.line_to((8.0, 56.0));
    scene.push_vector(VectorItem::stroked(
        VectorId(1),
        Arc::new(opposite),
        stroke,
        6.0,
    ));

    // A rounded icon: a circle inside a rounded clip whose corners it meets.
    let clip = scene.clips.only(ClipLink::rounded(
        rect(72.0, 8.0, 48.0, 48.0),
        Vec2::splat(DevicePx(12.0)),
    ));
    vector(
        &mut scene,
        2,
        circle(96.0, 32.0, 22.0),
        opaque(255, 255, 255),
        clip,
    );

    // A self-intersecting path: a five-pointed star, whose interior is decided by the fill rule and
    // whose crossings are where a rasteriser can double-count coverage.
    let mut star = kurbo::BezPath::new();
    for index in 0..5 {
        let angle =
            -std::f64::consts::FRAC_PI_2 + f64::from(index) * 4.0 * std::f64::consts::TAU / 10.0;
        let point = (64.0 + 40.0 * angle.cos(), 92.0 + 32.0 * angle.sin());
        if index == 0 {
            star.move_to(point);
        } else {
            star.line_to(point);
        }
    }
    star.close_path();
    let fill = solid(&mut scene, opaque(255, 255, 255));
    scene.push_vector(VectorItem::filled(VectorId(3), Arc::new(star), fill));

    scene.finish(&DamageSet::full());
    scene
}

/// A rasteriser asking for `method`, attached to `harness`.
fn with(harness: &mut support::Harness, method: vello::AaConfig) {
    let gpu = Arc::clone(harness.gpu());
    let mut raster = VelloRaster::new(&gpu, 128, 128).expect("a rasteriser");
    raster.set_antialiasing(method);
    harness.set_vector_raster(Box::new(raster));
}

/// Analytic area coverage and sixteen-sample coverage agree closely enough on the content that is
/// supposed to separate them, so the faster one is the default.
///
/// If this ever stops holding, the configuration changes here and once — which is the whole reason
/// this comparison happens before there is a corpus of recorded pictures to re-baseline.
#[test]
fn area_coverage_is_kept_because_the_multisampled_alternative_shows_nothing_it_hides() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let scene = provoking();

    with(&mut harness, vello::AaConfig::Area);
    let area = present(&mut harness.renderer, &scene);
    with(&mut harness, vello::AaConfig::Msaa16);
    let multisampled = present(&mut harness.renderer, &scene);

    let mut worst = 0u8;
    let mut differing = 0u32;
    let mut worst_at = (0, 0);
    for y in 0..area.size().height {
        for x in 0..area.size().width {
            let (a, b) = (area.rgba(x, y), multisampled.rgba(x, y));
            let error = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
            if error > worst {
                worst = error;
                worst_at = (x, y);
            }
            if error > 0 {
                differing += 1;
            }
        }
    }
    println!("area vs msaa16: worst {worst} at {worst_at:?}, differing {differing}");

    // A conflation artefact is a *whole* seam or a *whole* doubled region, not an edge level: it
    // would show as a run of pixels differing by most of the range. Nothing here does.
    assert!(
        worst <= 64,
        "the two coverage methods differ by {worst} at {worst_at:?}, which is a visible artefact \
         rather than an edge level — the configuration should change"
    );
    // Measured on this device: worst 48, over 845 pixels — the outlines of four shapes.
    assert!(
        differing < 1_200,
        "{differing} pixels differ between the two coverage methods, which is more than the \
         outlines in this scene have edge pixels"
    );
    // And they are genuinely two different methods, so this is a comparison rather than a
    // restatement of one number.
    assert!(differing > 0);

    // Where the two strokes cross, both must read as fully covered rather than as a seam.
    for method in [&area, &multisampled] {
        assert_eq!(
            method.rgba(32, 32),
            [255, 255, 255, 255],
            "the crossing of two strokes came out as a seam"
        );
    }
}

/// The self-intersecting path obeys the fill rule it was given, under either coverage method.
#[test]
fn a_self_intersecting_path_fills_by_the_rule_it_was_given() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let star = {
        let mut path = kurbo::BezPath::new();
        for index in 0..5 {
            let angle = -std::f64::consts::FRAC_PI_2
                + f64::from(index) * 4.0 * std::f64::consts::TAU / 10.0;
            let point = (64.0 + 48.0 * angle.cos(), 64.0 + 48.0 * angle.sin());
            if index == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        path.close_path();
        Arc::new(path)
    };

    let mut non_zero = scene();
    quad(&mut non_zero, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    let fill = solid(&mut non_zero, opaque(255, 255, 255));
    non_zero.push_vector(VectorItem::filled(VectorId(0), Arc::clone(&star), fill));
    non_zero.finish(&DamageSet::full());
    let filled = present(&mut harness.renderer, &non_zero);

    let mut even_odd = scene();
    quad(&mut even_odd, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    let fill = solid(&mut even_odd, opaque(255, 255, 255));
    even_odd.push_vector(VectorItem::filled(VectorId(0), star, fill).even_odd());
    even_odd.finish(&DamageSet::full());
    let holed = present(&mut harness.renderer, &even_odd);

    // The pentagon in the middle of a five-pointed star is wound twice, so the non-zero rule fills
    // it and the even-odd rule leaves it empty. A rasteriser that ignored the rule would produce
    // the same picture twice.
    assert_eq!(filled.rgba(64, 64), [255, 255, 255, 255]);
    assert_eq!(holed.rgba(64, 64), [0, 0, 0, 255]);
    // And a point in a limb of the star is inside under both rules.
    assert_eq!(filled.rgba(64, 24), [255, 255, 255, 255]);
    assert_eq!(holed.rgba(64, 24), [255, 255, 255, 255]);
}

/// A stroke is drawn, and it is drawn where the stroke is rather than where the line is.
#[test]
fn a_stroke_covers_its_own_width() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    let mut line = kurbo::BezPath::new();
    line.move_to((64.0, 16.0));
    line.line_to((64.0, 112.0));
    let paint = solid(&mut scene, opaque(255, 255, 255));
    scene.push_vector(VectorItem::stroked(VectorId(0), Arc::new(line), paint, 8.0));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut harness.renderer, &scene);

    assert_eq!(pixels.rgba(64, 64), [255, 255, 255, 255], "on the line");
    assert_eq!(
        pixels.rgba(61, 64),
        [255, 255, 255, 255],
        "three either side"
    );
    assert_eq!(pixels.rgba(67, 64), [255, 255, 255, 255]);
    assert_eq!(pixels.rgba(54, 64), [0, 0, 0, 255], "and not ten away");
    // A path with a stroke and no fill paints only the stroke, so the shape's interior above the
    // line's start is untouched.
    assert_eq!(pixels.rgba(64, 8), [0, 0, 0, 255]);
    let _ = PaintRef::NONE;
    let _ = ClipId::ROOT;
}
