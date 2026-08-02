//! Path geometry, under the same number policy as everything else.
//!
//! The path library's own SVG rendering is deterministic but not *stable*: it prints every
//! significant digit, so a coordinate that arrived at 64 by subtraction renders as
//! `63.99999999999999` and diffs against the same point reached by addition. Rendering the elements
//! here puts the geometry through [`float`] with everything else.

use zgui_scene::kurbo::{BezPath, PathEl, Point};

use crate::text::number::float;

/// One path, as SVG-shaped commands with stable coordinates.
pub fn of(path: &BezPath) -> String {
    let mut text = String::new();
    for element in path.elements() {
        if !text.is_empty() {
            text.push(' ');
        }
        match element {
            PathEl::MoveTo(to) => text.push_str(&format!("M {}", point(*to))),
            PathEl::LineTo(to) => text.push_str(&format!("L {}", point(*to))),
            PathEl::QuadTo(control, to) => {
                text.push_str(&format!("Q {} {}", point(*control), point(*to)));
            }
            PathEl::CurveTo(first, second, to) => text.push_str(&format!(
                "C {} {} {}",
                point(*first),
                point(*second),
                point(*to)
            )),
            PathEl::ClosePath => text.push('Z'),
        }
    }
    text
}

/// One point.
fn point(point: Point) -> String {
    format!("{},{}", float(point.x as f32), float(point.y as f32))
}

#[cfg(test)]
mod tests {
    use zgui_scene::kurbo::{BezPath, Shape};

    use super::of;

    #[test]
    fn every_command_has_a_rendering() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((10.0, 0.0));
        path.quad_to((10.0, 5.0), (5.0, 5.0));
        path.curve_to((4.0, 4.0), (3.0, 3.0), (0.0, 0.0));
        path.close_path();
        assert_eq!(of(&path), "M 0,0 L 10,0 Q 10,5 5,5 C 4,4 3,3 0,0 Z");
    }

    #[test]
    fn a_coordinate_that_arrived_by_a_different_route_renders_the_same() {
        // The whole reason this exists rather than the path library's own rendering: two paths that
        // describe the same shape must produce the same text.
        let mut exact = BezPath::new();
        exact.move_to((64.0, 0.0));
        let mut accumulated = BezPath::new();
        accumulated.move_to((64.0 - f64::EPSILON, 0.0));
        assert_eq!(of(&exact), of(&accumulated));
    }

    #[test]
    fn a_difference_a_pixel_could_see_still_renders_differently() {
        let circle = zgui_scene::kurbo::Circle::new((0.0, 0.0), 4.0).to_path(0.1);
        let bigger = zgui_scene::kurbo::Circle::new((0.0, 0.0), 5.0).to_path(0.1);
        assert_ne!(of(&circle), of(&bigger));
    }
}
