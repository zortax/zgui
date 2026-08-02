//! Moving a document from its own coordinates into the ones it is drawn in.

use std::sync::Arc;

use crate::document::gradient::{Gradient, GradientKind};
use crate::document::shape::{Clip, Fill, Paint, Shape, Stroke};

/// How far a matrix stretches the horizontal and the vertical axis.
///
/// The length of each of the two mapped basis vectors. It is what a radius has to be scaled by
/// when a ramp defined by one radius passes through a matrix that scales the axes differently, and
/// it reports one for a rotation, which a matrix's coefficients on their own do not.
pub(crate) fn axis_scales(matrix: kurbo::Affine) -> (f64, f64) {
    let [a, b, c, d, _, _] = matrix.as_coeffs();
    ((a * a + b * b).sqrt(), (c * c + d * d).sqrt())
}

/// The uniform scale a matrix applies, as the square root of the area it multiplies by.
///
/// What a stroke width is scaled by. A matrix that scales the two axes differently has no single
/// stroke width, and this takes the one that preserves the area the stroke covers.
pub(crate) fn uniform_scale(matrix: kurbo::Affine) -> f64 {
    matrix.determinant().abs().sqrt()
}

/// One ramp, placed.
fn gradient(source: &Gradient, matrix: kurbo::Affine) -> Gradient {
    let kind = match source.kind {
        GradientKind::Linear { start, end } => GradientKind::Linear {
            start: matrix * start,
            end: matrix * end,
        },
        GradientKind::Radial {
            center,
            radius_x,
            radius_y,
        } => {
            let (x, y) = axis_scales(matrix);
            GradientKind::Radial {
                center: matrix * center,
                radius_x: radius_x * x,
                radius_y: radius_y * y,
            }
        }
    };
    Gradient {
        kind,
        stops: source.stops.clone(),
        repeating: source.repeating,
    }
}

/// One paint, placed. Colours do not move; ramps do.
fn paint(source: &Paint, matrix: kurbo::Affine) -> Paint {
    match source {
        Paint::Solid(ink) => Paint::Solid(*ink),
        Paint::Gradient(ramp) => Paint::Gradient(gradient(ramp, matrix)),
    }
}

/// One outline, with everything about it, placed.
pub fn shape(source: &Shape, matrix: kurbo::Affine) -> Shape {
    let scale = uniform_scale(matrix);
    Shape {
        path: Arc::new(matrix * source.path.as_ref().clone()),
        fill: source.fill.as_ref().map(|fill| Fill {
            paint: paint(&fill.paint, matrix),
            rule: fill.rule,
        }),
        stroke: source.stroke.as_ref().map(|stroke| Stroke {
            paint: paint(&stroke.paint, matrix),
            style: scaled(&stroke.style, scale),
        }),
        clips: source
            .clips
            .iter()
            .map(|clip| Clip {
                path: Arc::new(matrix * clip.path.as_ref().clone()),
                rule: clip.rule,
            })
            .collect(),
    }
}

/// A stroke style at `scale` of its size.
///
/// Every length in it, not only the width: a dash pattern that did not scale with the drawing
/// would turn one icon into a different picture at every size it is drawn at.
fn scaled(style: &kurbo::Stroke, scale: f64) -> kurbo::Stroke {
    let mut scaled = style.clone();
    scaled.width *= scale;
    scaled.dash_offset *= scale;
    for dash in &mut scaled.dash_pattern {
        *dash *= scale;
    }
    scaled
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use smallvec::smallvec;
    use zgui_color::Color;

    use super::shape;
    use crate::document::gradient::{Gradient, GradientKind, Stop};
    use crate::document::ink::Ink;
    use crate::document::shape::{Paint, Shape, Stroke};

    fn line() -> Arc<kurbo::BezPath> {
        let mut path = kurbo::BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((10.0, 0.0));
        Arc::new(path)
    }

    #[test]
    fn a_scaled_drawing_is_drawn_thicker_and_its_dashes_are_longer() {
        let source = Shape {
            path: line(),
            fill: None,
            stroke: Some(Stroke {
                paint: Paint::Solid(Ink::Solid(Color::BLACK)),
                style: kurbo::Stroke::new(2.0).with_dashes(1.0, [4.0, 4.0]),
            }),
            clips: Vec::new(),
        };
        let placed = shape(&source, kurbo::Affine::scale(3.0));
        let stroke = placed.stroke.expect("a stroke stays a stroke");
        assert!((stroke.style.width - 6.0).abs() < 1.0e-9);
        assert!((stroke.style.dash_offset - 3.0).abs() < 1.0e-9);
        assert_eq!(stroke.style.dash_pattern.as_slice(), &[12.0, 12.0]);
    }

    #[test]
    fn a_ramp_moves_with_the_shape_it_paints_rather_than_staying_where_it_was() {
        let ramp = Gradient::padded(
            GradientKind::Linear {
                start: kurbo::Point::new(0.0, 0.0),
                end: kurbo::Point::new(10.0, 0.0),
            },
            smallvec![
                Stop {
                    offset: 0.0,
                    color: Ink::Solid(Color::BLACK),
                },
                Stop {
                    offset: 1.0,
                    color: Ink::Solid(Color::WHITE),
                },
            ],
        );
        let source = Shape {
            path: line(),
            fill: Some(crate::document::shape::Fill {
                paint: Paint::Gradient(ramp),
                rule: peniko::Fill::NonZero,
            }),
            stroke: None,
            clips: Vec::new(),
        };
        let placed = shape(
            &source,
            kurbo::Affine::translate((100.0, 0.0)) * kurbo::Affine::scale(2.0),
        );
        let Some(Paint::Gradient(moved)) = placed.fill.map(|fill| fill.paint) else {
            panic!("a ramp stays a ramp");
        };
        assert_eq!(
            moved.kind,
            GradientKind::Linear {
                start: kurbo::Point::new(100.0, 0.0),
                end: kurbo::Point::new(120.0, 0.0),
            }
        );
    }
}
