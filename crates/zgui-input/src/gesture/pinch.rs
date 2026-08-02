//! Two contacts moving towards or away from each other.

use zgui_geom::{Css, CssPx, Point};

/// Below this many CSS pixels apart, two contacts have no meaningful separation to scale from.
///
/// Two fingers touching produce a distance near zero, and dividing by it turns the first millimetre
/// of movement into a scale factor of several hundred.
const MEANINGFUL: f32 = 1.0;

/// How far apart two contacts are, in CSS pixels.
pub fn distance(first: Point<CssPx, Css>, second: Point<CssPx, Css>) -> f32 {
    let dx = second.x.0 - first.x.0;
    let dy = second.y.0 - first.y.0;
    (dx * dx + dy * dy).sqrt()
}

/// How much two contacts have spread since they were `origin` apart, or nothing when they started
/// too close together to say.
pub fn scale(origin: f32, now: f32) -> Option<f32> {
    (origin >= MEANINGFUL).then(|| now / origin)
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Css, CssPx, Point};

    use super::{distance, scale};

    fn at(x: f32, y: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    #[test]
    fn the_distance_is_the_ordinary_one() {
        assert_eq!(distance(at(0.0, 0.0), at(3.0, 4.0)), 5.0);
        assert_eq!(distance(at(3.0, 4.0), at(0.0, 0.0)), 5.0);
    }

    #[test]
    fn spreading_apart_scales_up_and_closing_scales_down() {
        assert_eq!(scale(100.0, 200.0), Some(2.0));
        assert_eq!(scale(100.0, 50.0), Some(0.5));
    }

    #[test]
    fn two_contacts_that_started_on_top_of_each_other_report_no_scale() {
        assert_eq!(scale(0.0, 40.0), None);
        assert_eq!(scale(0.4, 40.0), None);
    }
}
