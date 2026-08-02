//! Outlines, flattened into the line segments a sample is tested against.

use kurbo::{Affine, BezPath, PathEl, Point, Shape as _, Stroke, StrokeOpts};

/// How far a flattened polyline may stray from the curve it stands for, in device pixels.
///
/// A quarter of a pixel is well under the sixteenth of a pixel the sampling grid can resolve, so
/// flattening is not what limits the edge quality here.
pub const TOLERANCE: f64 = 0.1;

/// One line segment of an outline: two endpoints, in the space the samples are taken in.
pub type Segment = [f32; 4];

/// Appends `path`'s outline to `into` as closed polylines, transformed by `transform`.
///
/// Every subpath is closed whether or not it was written closed, because the winding test a sample
/// runs is only defined on closed outlines: an open one would leave a ray able to escape through the
/// gap and the interior would be undecided.
pub fn flatten(path: &BezPath, transform: Affine, into: &mut Vec<Segment>) {
    let mut start: Option<Point> = None;
    let mut cursor = Point::ZERO;
    kurbo::flatten(path.iter(), TOLERANCE, |element| match element {
        PathEl::MoveTo(point) => {
            close(start, cursor, transform, into);
            start = Some(point);
            cursor = point;
        }
        PathEl::LineTo(point) => {
            push(cursor, point, transform, into);
            cursor = point;
        }
        PathEl::ClosePath => {
            if let Some(first) = start {
                push(cursor, first, transform, into);
                cursor = first;
            }
            start = None;
        }
        // `flatten` yields only these three.
        PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
    });
    close(start, cursor, transform, into);
}

/// The outline of `path` stroked in `style`, flattened.
///
/// A stroke becomes an outline on the host and is then filled like any other, because a sample test
/// answers "is this point inside this outline" and a centre line is not an outline. The whole style
/// is expanded and not the width alone: caps, joins, the miter limit and the dash pattern are all
/// part of the outline a stroke stands for, and a stroker given only the width would draw a dashed
/// round-capped line as a solid butt-capped one.
pub fn flatten_stroke(path: &BezPath, style: &Stroke, transform: Affine, into: &mut Vec<Segment>) {
    let widened = Stroke {
        width: style.width.max(0.0),
        ..style.clone()
    };
    let stroked = kurbo::stroke(
        path.path_elements(TOLERANCE),
        &widened,
        &StrokeOpts::default(),
        TOLERANCE,
    );
    flatten(&stroked, transform, into);
}

/// Closes an open subpath back to where it began.
fn close(start: Option<Point>, cursor: Point, transform: Affine, into: &mut Vec<Segment>) {
    if let Some(first) = start
        && first != cursor
    {
        push(cursor, first, transform, into);
    }
}

/// Appends one segment, dropping the horizontal ones a crossing test would never count anyway.
fn push(from: Point, to: Point, transform: Affine, into: &mut Vec<Segment>) {
    let from = transform * from;
    let to = transform * to;
    if from.y == to.y {
        return;
    }
    into.push([from.x as f32, from.y as f32, to.x as f32, to.y as f32]);
}

#[cfg(test)]
mod tests {
    use kurbo::{Affine, BezPath, Circle, Shape as _, Stroke};

    use super::{flatten, flatten_stroke};

    #[test]
    fn an_unclosed_subpath_is_closed_before_it_becomes_segments() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((10.0, 0.0));
        path.line_to((10.0, 10.0));
        let mut segments = Vec::new();
        flatten(&path, Affine::IDENTITY, &mut segments);
        // The horizontal edge is dropped, so what is left is the right edge and the closing
        // diagonal — and the outline is closed, which is what makes an inside/outside test defined.
        assert_eq!(segments.len(), 2);
        let ends: Vec<[f32; 2]> = segments.iter().map(|s| [s[2], s[3]]).collect();
        assert!(ends.contains(&[0.0, 0.0]), "{ends:?}");
    }

    #[test]
    fn a_stroke_becomes_the_outline_of_the_stroke_rather_than_its_centre_line() {
        let line = {
            let mut path = BezPath::new();
            path.move_to((0.0, 0.0));
            path.line_to((0.0, 20.0));
            path
        };
        let mut centre = Vec::new();
        flatten(&line, Affine::IDENTITY, &mut centre);
        let mut outline = Vec::new();
        flatten_stroke(&line, &Stroke::new(4.0), Affine::IDENTITY, &mut outline);
        assert!(
            outline.len() > centre.len(),
            "a stroked line has two sides and two caps, not one segment"
        );
        let widest = outline
            .iter()
            .flat_map(|s| [s[0], s[2]])
            .fold(f32::MIN, f32::max);
        assert!(
            (widest - 2.0).abs() < 0.01,
            "a four-wide stroke reaches two either side of its centre, not {widest}"
        );
    }

    /// A dash pattern is part of the outline a stroke stands for, not a decoration on top of it.
    #[test]
    fn a_dashed_stroke_is_a_different_outline_from_the_solid_one() {
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((0.0, 40.0));

        let mut solid = Vec::new();
        flatten_stroke(&line, &Stroke::new(4.0), Affine::IDENTITY, &mut solid);
        let mut dashed = Vec::new();
        flatten_stroke(
            &line,
            &Stroke::new(4.0).with_dashes(0.0, [4.0, 4.0]),
            Affine::IDENTITY,
            &mut dashed,
        );
        assert!(
            dashed.len() > solid.len(),
            "five dashes have more edges than one line: {} against {}",
            dashed.len(),
            solid.len()
        );
    }

    #[test]
    fn a_curve_is_flattened_finely_enough_to_be_a_circle() {
        let circle = Circle::new((0.0, 0.0), 50.0).into_path(0.01);
        let mut segments = Vec::new();
        flatten(&circle, Affine::IDENTITY, &mut segments);
        let radius = segments
            .iter()
            .map(|s| (s[0] * s[0] + s[1] * s[1]).sqrt())
            .fold(f32::MIN, f32::max);
        assert!((radius - 50.0).abs() < 0.25, "{radius}");
    }
}
