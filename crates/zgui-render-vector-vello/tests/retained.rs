//! What is kept between frames, and what has to be made again.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::Affine2;
use zgui_render::{VectorFrame, VectorRaster};
use zgui_render_vector_coverage::CoverageRaster;
use zgui_render_vector_vello::VelloRaster;
use zgui_scene::{ClipId, Scene, VectorId, VectorItem};

use support::{Which, harness, opaque, path, present, quad, rect, scene, solid, vector};

/// Runs one frame of `scene` through `raster` without a renderer.
fn rasterise(raster: &mut impl VectorRaster, scene: &Scene) {
    let plan = raster.plan(scene.pass_plan());
    if plan.is_empty() {
        return;
    }
    raster.clear_targets(&plan);
    let placements = zgui_scene::Placements::of(&scene.spatial);
    let mut frame = VectorFrame::new(
        &plan,
        &scene.primitives.vectors,
        &scene.clips,
        &scene.paints,
        &placements,
    );
    raster.prepare(&mut frame).expect("nothing failed");
}

/// A scene of one square, drawn at `x`.
fn one_square(x: f32, colour: [u8; 3]) -> Scene {
    let mut scene = scene();
    vector(
        &mut scene,
        0,
        path(rect(x, 16.0, 32.0, 32.0)),
        opaque(colour[0], colour[1], colour[2]),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    scene
}

/// An item whose geometry and paint have not changed is re-placed rather than encoded again.
///
/// That is the whole reason a rasteriser keeps anything between frames: placing a cached encoding
/// is a copy, and making one again is a re-encoding of every curve in it.
#[test]
fn an_unchanged_item_is_re_placed_rather_than_encoded_again() {
    let Some(harness) = harness(Which::Vello) else {
        return;
    };
    let gpu = Arc::clone(harness.gpu());
    let mut raster = VelloRaster::new(&gpu, 128, 128).expect("a rasteriser");

    let first = one_square(16.0, [255, 0, 0]);
    rasterise(&mut raster, &first);
    let (held, (hits, misses)) = raster.cache();
    assert_eq!(held, 1);
    assert_eq!((hits, misses), (0, 1), "the first frame has to encode it");

    // The same geometry and the same paint: nothing to encode again.
    rasterise(&mut raster, &first);
    let (_, (hits, misses)) = raster.cache();
    assert_eq!((hits, misses), (1, 1));

    // A different colour under the same identity is a different encoding, because the brush is
    // encoded into it.
    let recoloured = one_square(16.0, [0, 255, 0]);
    rasterise(&mut raster, &recoloured);
    let (held, (hits, misses)) = raster.cache();
    assert_eq!((hits, misses), (1, 2), "a recoloured item is encoded again");
    assert_eq!(
        held, 2,
        "and the old encoding stays until the byte cap asks for its room"
    );
}

/// An item whose placement alone moved is placed from the encoding it already has.
///
/// The placement is what a scroll and a moved replay change, so this is what keeps a scrolled
/// drawing from being encoded again. The moved picture has to be the one a fresh rasteriser draws.
#[test]
fn a_moved_placement_costs_no_re_encoding() {
    let Some((mut moved, mut fresh)) = support::twins(support::SIDE, Which::Vello) else {
        return;
    };
    let gpu = Arc::clone(moved.gpu());
    let mut raster = VelloRaster::new(&gpu, 128, 128).expect("a rasteriser");
    let geometry = support::circle(0.0, 0.0, 12.0);
    let placed = |x: f64| {
        let mut scene = scene();
        quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
        let fill = solid(&mut scene, opaque(255, 255, 255));
        let item = VectorItem::filled(VectorId(0), Arc::clone(&geometry), fill)
            .placed(kurbo::Affine::translate((x, 40.0)) * kurbo::Affine::scale(1.5));
        scene.push_vector(item);
        scene.finish(&DamageSet::full());
        scene
    };

    let (left, right) = (placed(24.0), placed(80.25));
    rasterise(&mut raster, &left);
    rasterise(&mut raster, &right);
    let (_, (hits, misses)) = raster.cache();
    assert_eq!(
        (hits, misses),
        (1, 1),
        "the moved placement was encoded again"
    );

    present(&mut moved.renderer, &left);
    let after = present(&mut moved.renderer, &right);
    let expected = present(&mut fresh, &right);
    assert_eq!(
        support::difference(support::SIDE, &after, &expected),
        None,
        "a moved placement draws what a fresh encoding draws"
    );
}

/// A gradient in path space under a placement draws what the placed path and the placed gradient
/// draw.
#[test]
fn a_placed_gradient_matches_the_placed_path() {
    let Some((mut through, mut placed)) = support::twins(support::SIDE, Which::Vello) else {
        return;
    };
    let placement = kurbo::Affine::translate((20.0, 30.0)) * kurbo::Affine::scale(4.0);
    let source = path(rect(0.0, 0.0, 20.0, 12.0));
    let ramp = |scene: &mut Scene, start: (f64, f64), end: (f64, f64)| {
        let point = |(x, y): (f64, f64)| {
            zgui_geom::Point::new(zgui_geom::DevicePx(x as f32), zgui_geom::DevicePx(y as f32))
        };
        scene.paints.add(zgui_scene::Paint::Gradient {
            kind: zgui_scene::GradientKind::Linear {
                start: point(start),
                end: point(end),
            },
            stops: [
                zgui_color::GradientStop::new(0.0, opaque(255, 0, 0)),
                zgui_color::GradientStop::new(1.0, opaque(0, 0, 255)),
            ]
            .into_iter()
            .collect(),
            space: zgui_color::ColorSpace::Srgb,
            hue: zgui_color::HueInterpolation::Shorter,
            repeating: false,
        })
    };

    let mut in_path_space = scene();
    let fill = ramp(&mut in_path_space, (0.0, 0.0), (20.0, 0.0));
    in_path_space
        .push_vector(VectorItem::filled(VectorId(0), Arc::clone(&source), fill).placed(placement));
    in_path_space.finish(&DamageSet::full());

    let mut in_device_space = scene();
    let start = placement * kurbo::Point::new(0.0, 0.0);
    let end = placement * kurbo::Point::new(20.0, 0.0);
    let fill = ramp(&mut in_device_space, (start.x, start.y), (end.x, end.y));
    in_device_space.push_vector(VectorItem::filled(
        VectorId(0),
        Arc::new(placement * source.as_ref().clone()),
        fill,
    ));
    in_device_space.finish(&DamageSet::full());

    let one = present(&mut through.renderer, &in_path_space);
    let other = present(&mut placed, &in_device_space);
    assert!(
        one.rgba(60, 50) != [0, 0, 0, 0],
        "the placed gradient drew nothing inside its shape"
    );
    let worst = support::difference(support::SIDE, &one, &other);
    assert!(
        worst.is_none_or(|(_, _, delta)| delta <= 1),
        "the placed gradient differs from the placed path by {worst:?}"
    );
}

/// A moved item keeps its encoding, because where it is drawn is not part of it.
#[test]
fn moving_an_item_costs_no_re_encoding() {
    let Some(harness) = harness(Which::Vello) else {
        return;
    };
    let gpu = Arc::clone(harness.gpu());
    let mut raster = VelloRaster::new(&gpu, 128, 128).expect("a rasteriser");

    let geometry = path(rect(0.0, 0.0, 32.0, 32.0));
    let place = |x: f32| {
        let mut scene = scene();
        let fill = solid(&mut scene, opaque(255, 255, 255));
        let mut item = VectorItem::filled(VectorId(0), Arc::clone(&geometry), fill);
        item.transform = Some(space(
            &mut scene,
            2,
            Affine2::translation(x, 16.0).to_matrix4(),
        ));
        scene.push_vector(item);
        scene.finish(&DamageSet::full());
        scene
    };

    let left = place(8.0);
    rasterise(&mut raster, &left);
    let right = place(64.0);
    rasterise(&mut raster, &right);
    let (_, (hits, misses)) = raster.cache();
    assert_eq!(
        (hits, misses),
        (1, 1),
        "an item that only moved was encoded once and placed twice"
    );
}

/// The item's own transform is applied, and it is applied where the item is drawn.
#[test]
fn an_items_transform_moves_what_it_draws() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    let fill = solid(&mut scene, opaque(255, 255, 255));
    let mut item = VectorItem::filled(VectorId(0), path(rect(0.0, 0.0, 24.0, 24.0)), fill);
    item.transform = Some(space(
        &mut scene,
        2,
        Affine2::translation(64.0, 64.0).to_matrix4(),
    ));
    // The ink is the display list's business, and it is what the composite reads, so it has to be
    // stated in the space the item ends up in.
    item.ink = rect(64.0, 64.0, 24.0, 24.0);
    scene.push_vector(item);
    scene.finish(&DamageSet::full());

    let pixels = present(&mut harness.renderer, &scene);
    assert_eq!(
        pixels.rgba(76, 76),
        [255, 255, 255, 255],
        "the item was not drawn where its transform puts it"
    );
    assert_eq!(
        pixels.rgba(12, 12),
        [0, 0, 0, 255],
        "and it was not also drawn where it would have been without one"
    );
}

/// A transform a two-dimensional scene cannot express is counted rather than half-applied.
#[test]
fn a_three_dimensional_transform_is_counted_and_the_item_is_still_drawn() {
    let Some(harness) = harness(Which::Vello) else {
        return;
    };
    let gpu = Arc::clone(harness.gpu());
    let mut raster = VelloRaster::new(&gpu, 128, 128).expect("a rasteriser");

    let mut scene = scene();
    let fill = solid(&mut scene, opaque(255, 255, 255));
    let mut item = VectorItem::filled(VectorId(0), path(rect(16.0, 16.0, 32.0, 32.0)), fill);
    // A perspective matrix: perfectly legitimate in a display list, and not something a
    // two-dimensional affine can carry.
    item.transform = Some(space(&mut scene, 3, zgui_geom::Matrix4::perspective(800.0)));
    scene.push_vector(item);
    scene.finish(&DamageSet::full());

    rasterise(&mut raster, &scene);
    assert_eq!(
        raster.last_frame().flattened_transforms,
        1,
        "a transform that could not be applied has to be counted, or it is lost silently"
    );
    assert_eq!(
        raster.last_frame().unpaintable,
        0,
        "and the item is still drawn: dropping it would delete content over a transform"
    );
    assert_eq!(raster.passes(), 1);
}

/// The fallback flattens the outlines it is given, and says how many segments that was.
#[test]
fn the_fallback_reports_what_it_flattened() {
    let Some(harness) = harness(Which::Vello) else {
        return;
    };
    let gpu = Arc::clone(harness.gpu());
    let mut raster = CoverageRaster::new(&gpu, 128, 128);

    let mut scene = scene();
    vector(
        &mut scene,
        0,
        support::circle(64.0, 64.0, 32.0),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    rasterise(&mut raster, &scene);

    let flattened = raster.last_frame().segments;
    assert!(
        flattened > 32,
        "a circle of radius thirty-two flattens to more than {flattened} segments at a tenth of a \
         pixel"
    );
    assert_eq!(raster.last_frame().unpaintable, 0);
    assert!(raster.memory().buffers > 0, "the segments reached a buffer");
}

/// A coordinate system directly under the viewport, holding `matrix`.
///
/// Named after a made-up owner, because a scene built by hand has no boxes to name one after.
fn space(scene: &mut Scene, owner: u64, matrix: zgui_geom::Matrix4) -> zgui_scene::SpatialId {
    let viewport = scene.spatial.viewport();
    let owner = zgui_scene::PropertyOwner::new(owner).expect("not the empty word");
    let own = zgui_scene::OwnSpace::of(Some(matrix), None, false);
    scene.spatial.space_of(viewport, owner, own)
}

/// A canvas panned to a new fit keeps the encodings of its general shapes.
///
/// The fit is the items' placement, so a pan changes no path and no paint: the second frame draws
/// every shape from the encoding the first one made.
#[test]
fn a_panned_canvas_keeps_its_general_encodings() {
    let Some(harness) = harness(Which::Vello) else {
        return;
    };
    let gpu = Arc::clone(harness.gpu());
    let mut raster = VelloRaster::new(&gpu, 128, 128).expect("a rasteriser");
    let ramp = zgui_svg::Paint::Gradient(zgui_svg::Gradient::padded(
        zgui_svg::GradientKind::Linear {
            start: kurbo::Point::new(0.0, 0.0),
            end: kurbo::Point::new(16.0, 0.0),
        },
        [
            zgui_svg::Stop {
                offset: 0.0,
                color: zgui_svg::Ink::Solid(opaque(255, 0, 0)),
            },
            zgui_svg::Stop {
                offset: 1.0,
                color: zgui_svg::Ink::Solid(opaque(0, 0, 255)),
            },
        ]
        .into_iter()
        .collect(),
    ));
    // Triangles, which no recognising route takes.
    let shapes: Vec<zgui_svg::Shape> = (0..3)
        .map(|index| {
            let x = f64::from(index) * 20.0;
            let mut path = kurbo::BezPath::new();
            path.move_to((x, 0.0));
            path.line_to((x + 16.0, 2.0));
            path.line_to((x + 5.0, 14.0));
            path.close_path();
            zgui_svg::Shape {
                path: Arc::new(path),
                fill: Some(zgui_svg::Fill {
                    paint: ramp.clone(),
                    rule: peniko::Fill::NonZero,
                }),
                stroke: None,
                clips: Vec::new(),
            }
        })
        .collect();
    let shapes: Arc<[zgui_svg::Shape]> = Arc::from(shapes);
    let drawn = |fit: kurbo::Affine| {
        let mut scene = scene();
        let drawing = zgui_paint::Drawing::fitted_shared(Arc::clone(&shapes), fit);
        let pushed = zgui_paint::emit::vector::draw_drawing(
            &mut scene,
            VectorId(1),
            &drawing,
            zgui_paint::emit::vector::ShapePaint {
                fill: opaque(255, 255, 255),
                stroke: None,
                stroke_width: 1.0,
            },
            &zgui_paint::content::NoVectorMasks,
            zgui_paint::emit::vector::VectorPlacement {
                clip: ClipId::ROOT,
                transform: zgui_scene::SpatialId::VIEWPORT,
                scale: 1.0,
            },
        );
        assert_eq!(pushed, 3);
        scene.finish(&DamageSet::full());
        scene
    };

    rasterise(
        &mut raster,
        &drawn(kurbo::Affine::translate((4.0, 8.0)) * kurbo::Affine::scale(1.5)),
    );
    let (_, (hits, misses)) = raster.cache();
    assert_eq!((hits, misses), (0, 3));
    rasterise(
        &mut raster,
        &drawn(kurbo::Affine::translate((-9.5, 30.25)) * kurbo::Affine::scale(1.5)),
    );
    let (_, (_, after)) = raster.cache();
    assert_eq!(after, misses, "the pan encoded a general shape again");
}
