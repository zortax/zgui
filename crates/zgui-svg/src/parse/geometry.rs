//! The parser's geometry, in the geometry the rest of the framework uses.

use usvg::tiny_skia_path::{Path, PathSegment, Point, Transform};

/// One parsed outline, as Béziers, with `placement` applied.
///
/// Applied here rather than kept beside the path because a shape's transform is the composition of
/// every group above it and nothing downstream needs those groups back: what a rasteriser wants is
/// one outline in one space, and what a caller wants to intersect against damage is a bounding box
/// of where it really is.
pub(crate) fn path(source: &Path, placement: kurbo::Affine) -> kurbo::BezPath {
    let mut built = kurbo::BezPath::new();
    let at = |point: Point| placement * kurbo::Point::new(f64::from(point.x), f64::from(point.y));
    for segment in source.segments() {
        match segment {
            PathSegment::MoveTo(to) => built.move_to(at(to)),
            PathSegment::LineTo(to) => built.line_to(at(to)),
            PathSegment::QuadTo(control, to) => built.quad_to(at(control), at(to)),
            PathSegment::CubicTo(first, second, to) => {
                built.curve_to(at(first), at(second), at(to));
            }
            PathSegment::Close => built.close_path(),
        }
    }
    built
}

/// The parser's matrix, as the one the rest of the framework uses.
///
/// The two are the same six numbers in a different order, and getting the order wrong transposes
/// every rotation and swaps every skew — which is why this conversion exists once.
pub(crate) fn affine(transform: Transform) -> kurbo::Affine {
    kurbo::Affine::new([
        f64::from(transform.sx),
        f64::from(transform.ky),
        f64::from(transform.kx),
        f64::from(transform.sy),
        f64::from(transform.tx),
        f64::from(transform.ty),
    ])
}

#[cfg(test)]
mod tests {
    use usvg::tiny_skia_path::{PathBuilder, Transform};

    use super::{affine, path};
    use crate::document::place::axis_scales;

    #[test]
    fn a_matrix_survives_the_crossing_with_its_rotation_the_right_way_round() {
        // A quarter turn: it must send the x axis to the y axis, not to the negative one.
        let quarter = Transform::from_row(0.0, 1.0, -1.0, 0.0, 0.0, 0.0);
        let converted = affine(quarter);
        let placed = converted * kurbo::Point::new(1.0, 0.0);
        assert!((placed.x - 0.0).abs() < 1.0e-9, "{placed:?}");
        assert!((placed.y - 1.0).abs() < 1.0e-9, "{placed:?}");
    }

    #[test]
    fn a_translation_reaches_the_outline_rather_than_being_kept_beside_it() {
        let mut builder = PathBuilder::new();
        builder.move_to(0.0, 0.0);
        builder.line_to(10.0, 0.0);
        let source = builder.finish().expect("two points is a path");
        let moved = path(&source, kurbo::Affine::translate((5.0, 7.0)));
        let bounds = kurbo::Shape::bounding_box(&moved);
        assert_eq!((bounds.x0, bounds.y0), (5.0, 7.0));
        assert_eq!((bounds.x1, bounds.y1), (15.0, 7.0));
    }

    #[test]
    fn the_axis_scales_of_an_uneven_matrix_are_its_two_axes_and_not_one_of_them() {
        let (x, y) = axis_scales(kurbo::Affine::scale_non_uniform(3.0, 7.0));
        assert!((x - 3.0).abs() < 1.0e-9);
        assert!((y - 7.0).abs() < 1.0e-9);
        // A rotation stretches neither axis, whatever it does to the coefficients.
        let (x, y) = axis_scales(kurbo::Affine::rotate(0.7));
        assert!((x - 1.0).abs() < 1.0e-9 && (y - 1.0).abs() < 1.0e-9);
    }
}
