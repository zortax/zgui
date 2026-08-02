//! Filters: the separable blur, what it does at an edge, and what a backdrop reads.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_render_wgpu::Pixels;
use zgui_scene::{BackdropFilter, Filter, GroupBoundary, Quad, Scene};

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

/// A black square of `side`, offset by `offset`, blurred by `deviation`, over white.
fn blurred_square(deviation: f32, offset: (f32, f32), side: f32) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    let bounds = (
        (SIDE as f32 - side) * 0.5 + offset.0,
        (SIDE as f32 - side) * 0.5 + offset.1,
        side,
        side,
    );
    let boundary = GroupBoundary::start(
        rect(bounds.0, bounds.1, bounds.2, bounds.3),
        1.0,
        zgui_scene::peniko::BlendMode::default(),
        [Filter::Blur(deviation)].into_iter().collect(),
    );
    scene.push_group(boundary.clone());
    quad(&mut scene, bounds, [0, 0, 0]);
    scene.push_group(boundary.end());
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn a_blurred_edge_crosses_mid_grey_where_css_says_and_not_where_linear_light_would() {
    // A gaussian over black-on-white is symmetric, so the sample on the original edge is the mean
    // of the two — and which mean depends entirely on the space the blur runs in. Averaging the
    // gamma-encoded values, which is what CSS specifies and what every browser does, gives sRGB
    // 128. Averaging in linear light and encoding the result gives 188. The whole reason this
    // renderer holds one colour encoding in every target is that the difference is this large and
    // no image comparison against a differently-blurred reference could see it.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    // The measurement is a symmetry rather than a single sample, because a single sample also
    // measures where the edge happens to fall within a pixel. A gaussian is symmetric, so the pair
    // of samples the same distance either side of the edge holds a fraction f of black and 1 - f
    // of white; averaging the encoded values makes the two read 255(1 - f) and 255f, which sum to
    // 255 whatever f is. Averaging in linear light and encoding afterwards makes them sum to
    // 255((1 - f)^(1/2.2) + f^(1/2.2)) — 372 at the midpoint, and never below 255 anywhere.
    for deviation in [4.0f32, 8.0] {
        let scene = blurred_square(deviation, (0.0, 0.0), 64.0);
        let pixels = present(&mut renderer, &scene);
        // The edge falls on the boundary between pixels 31 and 32, so the pair straddling it is
        // 31 - k and 32 + k: their centres are equidistant from it, which is what the symmetry
        // below is a statement about.
        for step in [1i32, 3, 5] {
            let outside = i32::from(pixels.rgba(31 - step, 64)[0]);
            let inside = i32::from(pixels.rgba(32 + step, 64)[0]);
            let sum = outside + inside;
            assert!(
                (sum - 255).abs() <= 12,
                "sigma {deviation} at {step} px out: {outside} and {inside} sum to {sum}, where a \
                 gamma-space blur sums to 255 and a linear-light one to about 372"
            );
        }
        // And the midpoint itself is nowhere near the linear-light answer.
        let edge = i32::from(pixels.rgba(32, 64)[0]);
        assert!(
            (edge - 188).abs() > 40,
            "sigma {deviation}: the edge read {edge}, which is the linear-light answer"
        );
    }
}

#[test]
fn a_blur_bleeds_outwards_towards_transparent_and_the_reach_matches_the_deviation() {
    // A content filter's target is transparent outside what the group painted, so the blur fades
    // towards transparency rather than towards whatever was behind it — the CSS edge behaviour,
    // and the reason the target is cleared whole rather than only where the composite lands.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let sharp = {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(SIDE, SIDE));
        quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
        quad(&mut scene, (32.0, 32.0, 64.0, 64.0), [0, 0, 0]);
        scene.finish(&DamageSet::full());
        scene
    };
    let unblurred = present(&mut renderer, &sharp);
    assert_eq!(unblurred.rgba(24, 64), [255, 255, 255, 255]);

    let scene = blurred_square(6.0, (0.0, 0.0), 64.0);
    let pixels = present(&mut renderer, &scene);
    // Eight pixels outside the box is well inside three deviations, so it is grey rather than
    // white; thirty is well outside, so it is white again.
    let inside_reach = pixels.rgba(24, 64)[0];
    let outside_reach = pixels.rgba(2, 64)[0];
    assert!(
        inside_reach < 240,
        "eight pixels out is inside the blur, and read {inside_reach}"
    );
    assert!(
        outside_reach >= 253,
        "thirty pixels out is beyond it, and read {outside_reach}"
    );
    assert!(
        pixels.rgba(64, 64)[0] <= 4,
        "the middle of a blurred solid square is still solid"
    );
}

#[test]
fn a_blurred_group_at_sub_pixel_offsets_keeps_its_halo_still() {
    // The half-resolution grid the blur runs on is anchored to the device origin rather than to the
    // blurred content, so moving the content by a fraction of a pixel moves the halo by that
    // fraction and no more. A grid anchored to the region would snap to a different pair of source
    // texels at each offset, and the halo would jump about a pixel per frame under animation.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    // What is measured is where the halo's midpoint sits, to a fraction of a pixel: a lattice
    // anchored to the blurred content would snap to a different pair of source texels as the
    // content crossed each half of a texel, so the midpoint would sit still and then jump two
    // pixels rather than following the content.
    let mut crossings: Vec<f32> = Vec::new();
    for step in 0..5 {
        let offset = step as f32 * 0.25;
        let scene = blurred_square(6.0, (offset, 0.0), 64.0);
        let pixels = present(&mut renderer, &scene);
        let profile: Vec<f32> = (16..48).map(|x| f32::from(pixels.rgba(x, 64)[0])).collect();
        crossings.push(crossing(&profile, 128.0) + 16.0);
    }
    for (step, crossed) in crossings.iter().enumerate() {
        let expected = crossings[0] + step as f32 * 0.25;
        assert!(
            (crossed - expected).abs() <= 0.3,
            "at a {} px offset the halo sits at {crossed:.2} rather than {expected:.2}",
            step as f32 * 0.25
        );
    }
    assert!(
        crossings[4] - crossings[0] > 0.6,
        "a whole pixel of motion did move the halo, so the comparison above is not vacuous"
    );
}

#[test]
fn a_backdrop_filter_reads_what_is_beneath_it_and_writes_only_its_own_bounds() {
    // A backdrop samples the composite so far rather than its own content, so what it frosts is
    // whatever was already drawn — and it composites only over what it writes, which is what gives
    // a frosted panel a defined edge instead of a soft fade past it.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    // A hard black-and-white edge down the middle for the filter to soften.
    quad(&mut scene, (0.0, 0.0, 64.0, SIDE as f32), [0, 0, 0]);
    scene.push_backdrop(BackdropFilter::new(
        rect(32.0, 32.0, 64.0, 64.0),
        [Filter::Blur(6.0)].into_iter().collect(),
    ));
    scene.finish(&DamageSet::full());

    let pixels = present(&mut renderer, &scene);
    let inside = pixels.rgba(64, 64)[0];
    let above = pixels.rgba(64, 16)[0];
    assert!(
        (inside as i32 - 128).abs() <= 12,
        "inside the panel the edge is softened to about mid grey, and read {inside}"
    );
    assert_eq!(
        above, 255,
        "one pixel outside the panel the edge is untouched"
    );
    assert_eq!(
        pixels.rgba(60, 16)[0],
        0,
        "and so is the black side of it above the panel"
    );
}

#[test]
fn a_drop_shadow_falls_where_it_was_offset_to_and_keeps_the_content_sharp() {
    // `drop-shadow()` casts the content's own alpha rather than its box, and it is drawn behind
    // the content rather than over it. Both are visible in one frame: the shadow shows past the
    // bottom-right of the square and the square's own edge is still hard.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
    let bounds = (32.0, 32.0, 48.0, 48.0);
    let boundary = GroupBoundary::start(
        rect(bounds.0, bounds.1, bounds.2, bounds.3),
        1.0,
        zgui_scene::peniko::BlendMode::default(),
        [Filter::DropShadow {
            offset_x: 12.0,
            offset_y: 12.0,
            blur: 3.0,
            color: [0.0, 0.0, 0.0, 1.0],
        }]
        .into_iter()
        .collect(),
    );
    scene.push_group(boundary.clone());
    quad(&mut scene, bounds, [255, 0, 0]);
    scene.push_group(boundary.end());
    scene.finish(&DamageSet::full());

    let pixels = present(&mut renderer, &scene);
    let content = pixels.rgba(56, 56);
    assert_eq!(
        [content[0], content[1], content[2]],
        [255, 0, 0],
        "the content is drawn over its own shadow, undimmed"
    );
    let shadow = pixels.rgba(86, 86);
    assert!(
        shadow[0] < 120 && shadow[1] < 120,
        "twelve pixels past the corner is inside the shadow, and read {shadow:?}"
    );
    assert_eq!(
        pixels.rgba(20, 20),
        [255, 255, 255, 255],
        "and the shadow does not fall the other way"
    );
}

#[test]
fn a_filter_before_a_blur_is_not_the_same_picture_as_the_same_filter_after_it() {
    // A colour map with a constant term does not commute with a convolution, so the chain runs in
    // the order it was written and a step before a blur costs a target of its own. If the order
    // were folded away the two frames below would be identical.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let render = |renderer: &mut zgui_render_wgpu::WgpuRenderer, filters: Vec<Filter>| -> Pixels {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(SIDE, SIDE));
        quad(&mut scene, (0.0, 0.0, SIDE as f32, SIDE as f32), [255; 3]);
        let bounds = (32.0, 32.0, 64.0, 64.0);
        let boundary = GroupBoundary::start(
            rect(bounds.0, bounds.1, bounds.2, bounds.3),
            1.0,
            zgui_scene::peniko::BlendMode::default(),
            filters.into_iter().collect(),
        );
        scene.push_group(boundary.clone());
        // Two shades rather than one, because a filter that is affine on colour commutes with a
        // convolution over a region of a single colour: the two orders would then agree and the
        // assertion below would be about nothing. What breaks the commutation is the clamp, and
        // the clamp only bites where the two shades land on opposite sides of it.
        quad(
            &mut scene,
            (bounds.0, bounds.1, 32.0, bounds.3),
            [26, 26, 26],
        );
        quad(
            &mut scene,
            (bounds.0 + 32.0, bounds.1, 32.0, bounds.3),
            [128, 128, 128],
        );
        scene.push_group(boundary.end());
        scene.finish(&DamageSet::full());
        present(renderer, &scene)
    };
    let before = render(
        &mut renderer,
        vec![Filter::Contrast(4.0), Filter::Blur(5.0)],
    );
    let after = render(
        &mut renderer,
        vec![Filter::Blur(5.0), Filter::Contrast(4.0)],
    );
    assert!(
        before.max_difference(&after) > 8,
        "the two orders produced the same picture, so the chain was folded out of order"
    );
    assert!(
        renderer.groups().peak() >= 2,
        "a map before a blur needs a target of its own"
    );
}

#[test]
fn a_backdrop_filter_damages_its_source_rect_and_not_the_window() {
    // What a backdrop reads has to be inside what the frame has already redrawn, so the damage set
    // has to contain the region it samples — which is its own bounds grown by the filter's reach,
    // and not the window. The expansion is the paint stage's, and this is the shape of it: the
    // damaged area grows by three deviations on each side and stops there.
    let panel: Rect<zgui_geom::DevicePx, Device> = rect(32.0, 32.0, 64.0, 64.0);
    let backdrop = BackdropFilter::new(panel, [Filter::Blur(6.0)].into_iter().collect());
    assert!(!backdrop.reads_only_what_it_writes());

    let mut damage = DamageSet::<4>::new();
    damage.absorb(Rect::new(Point::new(60, 60), Size::new(4, 4)));
    assert!(damage.intersects(source_rect(&backdrop)));
    damage.absorb(source_rect(&backdrop));

    let covered = damage.rects()[0];
    assert!(
        covered.contains_rect(source_rect(&backdrop)),
        "the expansion covers every pixel the filter reads"
    );
    assert_eq!(
        covered.size,
        Size::new(64 + 36, 64 + 36),
        "and it grows by three deviations on each side rather than to the window"
    );
    assert!(
        covered.size.width < SIDE && covered.size.height < SIDE,
        "a damaged backdrop is not a damaged window"
    );

    // A filter with no reach reads only what it writes, and is deliberately not expanded for.
    let flat = BackdropFilter::new(panel, [Filter::Saturate(1.8)].into_iter().collect());
    assert!(flat.reads_only_what_it_writes());
}

/// Where `profile` crosses `level`, in fractional samples, interpolating between the two it lies
/// between.
fn crossing(profile: &[f32], level: f32) -> f32 {
    for (index, pair) in profile.windows(2).enumerate() {
        let (high, low) = (pair[0], pair[1]);
        if high >= level && low <= level && high > low {
            return index as f32 + (high - level) / (high - low);
        }
    }
    panic!("the profile never crosses {level}: {profile:?}");
}

/// A backdrop's read extent as whole device pixels.
fn source_rect(backdrop: &BackdropFilter) -> Rect<i32, Device> {
    Rect::from_corners(
        Point::new(
            backdrop.source.left().0 as i32,
            backdrop.source.top().0 as i32,
        ),
        Point::new(
            backdrop.source.right().0 as i32,
            backdrop.source.bottom().0 as i32,
        ),
    )
}
