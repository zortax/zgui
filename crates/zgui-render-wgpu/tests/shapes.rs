//! What the shape shaders actually draw, on a real device.
//!
//! Each of these guards a generalisation the reference implementation did not make: a corner is a
//! pair of semi-axes rather than one radius, a dash runs round an elliptical corner by arc length
//! rather than by angle, and a shadow's blurred scanline follows the ellipse equation rather than
//! the circle's. Every one of them degenerates to the reference's own arithmetic when the two
//! semi-axes are equal, so the counterfactual — what a scalar radius would have drawn — is what
//! makes each assertion mean something.

mod support;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Corners, DevicePx, Size, Vec2};
use zgui_scene::prim::{BorderStyle, DecorationStyle};
use zgui_scene::{Decoration, Quad, Scene, Shadow};

use support::{SIDE, opaque, plain_renderer, present, rect};

/// Elliptical radii of `x` by `y` on every corner.
fn radii(x: f32, y: f32) -> Corners<Vec2<DevicePx>> {
    let corner = Vec2::new(DevicePx(x), DevicePx(y));
    Corners {
        top_left: corner,
        top_right: corner,
        bottom_right: corner,
        bottom_left: corner,
    }
}

/// A scene the size of the test surface.
fn scene() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene
}

#[test]
fn a_corner_is_an_ellipse_and_not_a_circle() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = scene();
    let fill = scene.paints.add(zgui_scene::Paint::Solid(opaque(0, 0, 0)));
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 100.0, 60.0), fill).with_radii(radii(20.0, 10.0)));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    // Inside the 20-by-10 ellipse and *outside* a circle of radius 20, which is the whole
    // difference between `border-radius: 20px / 10px` and `border-radius: 20px`.
    assert_eq!(
        pixels.rgba(5, 4)[3],
        255,
        "a point inside the ellipse and outside the circle must be painted"
    );
    // Outside both.
    assert_eq!(pixels.rgba(1, 1)[3], 0, "the corner is cut away");
    // Well inside.
    assert_eq!(pixels.rgba(50, 30)[3], 255, "the middle is filled");
    // Beyond the box.
    assert_eq!(pixels.rgba(110, 30)[3], 0, "outside the box");
}

#[test]
fn a_dashed_border_has_gaps_and_a_solid_one_does_not() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let ink = opaque(0, 0, 0);
    let box_ = rect(8.0, 8.0, 100.0, 40.0);

    let mut coverage = |style: BorderStyle| {
        let mut scene = scene();
        let stroke = scene.paints.add(zgui_scene::Paint::Solid(ink));
        scene.push_quad(
            Quad::filled(box_, zgui_scene::PaintRef::NONE)
                .with_radii(radii(20.0, 8.0))
                .with_border([4.0; 4], stroke, style),
        );
        scene.finish(&DamageSet::full());
        let pixels = present(&mut renderer, &scene);
        // Along the middle of the top border, between the two corners.
        (40..80).map(|x| pixels.rgba(x, 10)[3]).collect::<Vec<u8>>()
    };

    let solid = coverage(BorderStyle::Solid);
    assert!(
        solid.iter().all(|alpha| *alpha == 255),
        "a solid border has no gaps: {solid:?}"
    );

    let dashed = coverage(BorderStyle::Dashed);
    assert!(
        dashed.contains(&255),
        "a dashed border has dashes: {dashed:?}"
    );
    assert!(dashed.contains(&0), "a dashed border has gaps: {dashed:?}");

    let dotted = coverage(BorderStyle::Dotted);
    let dashed_ink: u32 = dashed.iter().map(|alpha| u32::from(*alpha)).sum();
    let dotted_ink: u32 = dotted.iter().map(|alpha| u32::from(*alpha)).sum();
    assert!(
        dotted_ink < dashed_ink,
        "dots cover less of a side than dashes do: {dotted_ink} against {dashed_ink}"
    );
}

#[test]
fn a_dash_pattern_continues_round_the_corner_rather_than_restarting_at_it() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = scene();
    let stroke = scene.paints.add(zgui_scene::Paint::Solid(opaque(0, 0, 0)));
    scene.push_quad(
        Quad::filled(rect(8.0, 8.0, 100.0, 60.0), zgui_scene::PaintRef::NONE)
            .with_radii(radii(24.0, 12.0))
            .with_border([4.0; 4], stroke, BorderStyle::Dashed),
    );
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    // Sampling the whole of the top-left corner's arc: a pattern laid out by angle rather than by
    // arc length bunches up on the short semi-axis, which shows as a run with no gap at all.
    let arc: Vec<u8> = (0..24)
        .map(|step| {
            let angle = std::f32::consts::FRAC_PI_2 * step as f32 / 23.0;
            let x = 8.0 + 24.0 - 24.0 * angle.cos();
            let y = 8.0 + 12.0 - 12.0 * angle.sin();
            pixels.rgba(x.round() as i32, y.round() as i32)[3]
        })
        .collect();
    assert!(
        arc.iter().any(|alpha| *alpha > 200),
        "the corner is drawn at all: {arc:?}"
    );
    assert!(
        arc.iter().any(|alpha| *alpha < 40),
        "the corner carries gaps rather than one unbroken run: {arc:?}"
    );
}

#[test]
fn a_blurred_shadow_falls_off_outwards_and_stops_where_it_says_it_does() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = scene();
    let element = rect(40.0, 40.0, 48.0, 48.0);
    scene.push_shadow(Shadow::drop_shadow(
        element,
        (0.0, 0.0),
        0.0,
        6.0,
        Color::srgb_u8(0, 0, 0, 255),
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    // An outer shadow is never painted within the box that casts it: a box with no fill of its
    // own is a hole in the page, and must not wear its own shadow as a wash over its interior.
    let middle = pixels.rgba(64, 64)[3];
    assert!(
        middle == 0,
        "the casting box's interior is unpainted: {middle}"
    );

    // Outwards from the right edge: strictly decreasing, and gone by three standard deviations.
    let profile: Vec<u8> = (0..24).map(|step| pixels.rgba(88 + step, 64)[3]).collect();
    for pair in profile.windows(2) {
        assert!(
            pair[0] >= pair[1],
            "the falloff is monotone outwards: {profile:?}"
        );
    }
    assert!(
        profile[0] > 100,
        "just outside the box is still shadow: {profile:?}"
    );
    assert!(
        *profile.last().expect("a profile was sampled") == 0,
        "beyond three standard deviations there is nothing left: {profile:?}"
    );
}

#[test]
fn an_inset_shadow_darkens_the_inside_edge_and_leaves_the_middle_alone() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = scene();
    let element = rect(24.0, 24.0, 80.0, 80.0);
    scene.push_shadow(Shadow::inset_shadow(
        element,
        (0.0, 0.0),
        0.0,
        6.0,
        Color::srgb_u8(0, 0, 0, 255),
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    let edge = pixels.rgba(26, 64)[3];
    let middle = pixels.rgba(64, 64)[3];
    let outside = pixels.rgba(8, 64)[3];
    assert!(
        edge > middle,
        "the inside edge is darker: {edge} vs {middle}"
    );
    assert!(middle < 20, "the middle is almost untouched: {middle}");
    assert_eq!(outside, 0, "an inset shadow paints nothing outside its box");
}

/// The rows and the ink of a decoration line drawn in `style`.
///
/// The line occupies `(8, 56)` to `(108, 68)` with a two-pixel stroke, so a style that inks the
/// whole rectangle, one that inks part of each row and one that inks two bands of rows are all
/// distinguishable from the same readback.
fn decoration_rows(renderer: &mut zgui_render_wgpu::WgpuRenderer, style: DecorationStyle) -> Rows {
    let mut scene = scene();
    scene.push_decoration(Decoration::new(
        rect(8.0, 56.0, 100.0, 12.0),
        2.0,
        opaque(0, 0, 0),
        style,
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(renderer, &scene);
    Rows(
        (54..72)
            .map(|y| (8..108).map(|x| pixels.rgba(x, y)[3] > 128).collect())
            .collect(),
    )
}

/// Eighteen rows of a hundred samples each, starting two pixels above the line's rectangle.
struct Rows(Vec<Vec<bool>>);

impl Rows {
    /// How many of `y`'s samples are inked. `y` is a device row.
    fn inked(&self, y: i32) -> usize {
        self.0[(y - 54) as usize].iter().filter(|on| **on).count()
    }

    /// How many times row `y` changes between inked and not, which separates dashes from dots.
    fn transitions(&self, y: i32) -> usize {
        self.0[(y - 54) as usize]
            .windows(2)
            .filter(|pair| pair[0] != pair[1])
            .count()
    }
}

#[test]
fn every_decoration_style_inks_a_different_part_of_its_rectangle() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };

    let solid = decoration_rows(&mut renderer, DecorationStyle::Solid);
    assert_eq!(solid.inked(62), 100, "a solid line has no gaps along it");
    assert_eq!(solid.inked(54), 0, "and nothing above its rectangle");
    assert_eq!(solid.inked(70), 0, "or below it");

    // Dashes and dots ink about half the run each — the discriminator is the period, which is
    // proportional to the stroke, so a dotted line breaks many more times across the same length.
    let dashed = decoration_rows(&mut renderer, DecorationStyle::Dashed);
    let dotted = decoration_rows(&mut renderer, DecorationStyle::Dotted);
    for (name, rows) in [("dashed", &dashed), ("dotted", &dotted)] {
        assert!(
            (30..70).contains(&rows.inked(62)),
            "a {name} line inks part of its run, not all or none: {}",
            rows.inked(62)
        );
    }
    assert!(
        dotted.transitions(62) > 2 * dashed.transitions(62),
        "dots break the run far more often than dashes: {} against {}",
        dotted.transitions(62),
        dashed.transitions(62)
    );

    // Two lines with a gap between them, which is the whole of what `double` means.
    let double = decoration_rows(&mut renderer, DecorationStyle::Double);
    assert_eq!(double.inked(57), 100, "the upper of the two lines");
    assert_eq!(double.inked(62), 0, "the gap between them");
    assert_eq!(double.inked(66), 100, "the lower of the two lines");

    // A wave is not a straight line: the rows it inks are not the same rows all the way along.
    let wavy = decoration_rows(&mut renderer, DecorationStyle::Wavy);
    let crest = (54..72).filter(|y| wavy.inked(*y) > 0).count();
    assert!(
        crest >= 4,
        "a wave inks rows across its rectangle rather than one band: {crest}"
    );
    let fullest = (54..72).map(|y| wavy.inked(y)).max().unwrap_or(0);
    assert!(
        (1..60).contains(&fullest),
        "and no row of a wave is a straight line across it: {fullest}"
    );
}

#[test]
fn a_clip_cuts_a_quad_and_rounds_its_own_corners() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = scene();
    let fill = scene.paints.add(zgui_scene::Paint::Solid(opaque(0, 0, 0)));
    let clip = scene.clips.only(zgui_scene::ClipLink::rounded(
        rect(20.0, 20.0, 60.0, 60.0),
        Vec2::new(DevicePx(20.0), DevicePx(20.0)),
    ));
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 120.0, 120.0), fill).clipped(clip));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    assert_eq!(pixels.rgba(50, 50)[3], 255, "inside the clip");
    assert_eq!(pixels.rgba(10, 50)[3], 0, "outside the clip rectangle");
    assert_eq!(
        pixels.rgba(22, 22)[3],
        0,
        "the clip's own rounded corner cuts the quad"
    );
}
