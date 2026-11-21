//! A gradient's centre and ending shape, from the cascade to the paint table.

mod support;

use support::{Element, Harness};
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_paint::Tables;
use zgui_paint::emit::paint::gradient_paint;
use zgui_paint::lower::background::{Extent, GradientShape, RadialEnding};
use zgui_paint::paints::{GradientKind, Paint};

/// The first background layer of `mark`, styled with `background`.
fn layer(background: &str) -> zgui_paint::lower::background::GradientSpec {
    let css = format!(
        "root {{ display: block; width: 200px; height: 100px }}
         mark {{ display: block; width: 200px; height: 100px; background-image: {background} }}"
    );
    let harness = Harness::new(
        Element::new("root").children(vec![Element::new("mark")]),
        Box::leak(css.into_boxed_str()),
    );
    let key = harness.box_of("mark");
    let style = zgui_paint::lower::lower(&harness.store().node(key).style, 1.0);
    style
        .background
        .layers
        .first()
        .cloned()
        .expect("one gradient layer")
}

/// The gradient a layer becomes over a 200 by 100 box at (10, 20).
fn kind(spec: &zgui_paint::lower::background::GradientSpec) -> GradientKind {
    let mut scene = Tables::new();
    let bounds: Rect<DevicePx, Device> = Rect::new(
        Point::new(DevicePx(10.0), DevicePx(20.0)),
        Size::new(DevicePx(200.0), DevicePx(100.0)),
    );
    let reference = gradient_paint(&mut scene, spec, bounds, 1.0).expect("a paint");
    match scene.paints.get(reference.id().expect("an entry")) {
        Some(Paint::Gradient { kind, .. }) => *kind,
        other => panic!("a gradient paint, found {other:?}"),
    }
}

#[test]
fn a_radial_gradient_keeps_its_position_and_its_ending_shape() {
    let spec = layer("radial-gradient(circle at 0% 0%, red, transparent 30%)");
    let GradientShape::Radial { ending, .. } = &spec.shape else {
        panic!("a radial gradient lowers to a radial shape");
    };
    assert_eq!(*ending, RadialEnding::CircleExtent(Extent::FarthestCorner));
    let GradientKind::Radial {
        center,
        radius_x,
        radius_y,
    } = kind(&spec)
    else {
        panic!("a radial paint");
    };
    assert_eq!(center, Point::new(DevicePx(10.0), DevicePx(20.0)));
    assert!((radius_x - 200.0f32.hypot(100.0)).abs() < 1e-3);
    assert_eq!(radius_x, radius_y);
}

#[test]
fn an_ellipse_with_radii_and_keywords_resolves_against_the_box() {
    let spec = layer("radial-gradient(40px 20% at right 25% top 10px, red, blue)");
    let GradientKind::Radial {
        center,
        radius_x,
        radius_y,
    } = kind(&spec)
    else {
        panic!("a radial paint");
    };
    assert_eq!(
        center,
        Point::new(DevicePx(10.0 + 150.0), DevicePx(20.0 + 10.0))
    );
    assert_eq!((radius_x, radius_y), (40.0, 20.0));

    let spec = layer("radial-gradient(closest-side at 50px 50%, red, blue)");
    let GradientKind::Radial {
        radius_x, radius_y, ..
    } = kind(&spec)
    else {
        panic!("a radial paint");
    };
    assert_eq!((radius_x, radius_y), (50.0, 50.0));
}

#[test]
fn a_conic_gradient_keeps_its_position() {
    let spec = layer("conic-gradient(from 90deg at 25% 75%, red, blue)");
    let GradientKind::Conic { center, from_angle } = kind(&spec) else {
        panic!("a conic paint");
    };
    assert_eq!(
        center,
        Point::new(DevicePx(10.0 + 50.0), DevicePx(20.0 + 75.0))
    );
    assert!((from_angle - core::f32::consts::FRAC_PI_2).abs() < 1e-5);
}
