//! Wheel events as detents or as a continuous stream.
//!
//! A browser reports a notched wheel and a touch surface through one event, and mostly in the same
//! unit. The two need opposite treatment: a detent is one whole step that the framework carries
//! over several frames, and a touch surface is a stream the system already animates, momentum
//! included. [`classify`] tells them apart.
//!
//! A detent in pixels keeps the distance the browser gave it, so one click of a wheel travels as
//! far here as in any other page.
//!
//! A positive delta moves the scroll offset right and down, in the browser and in the framework,
//! so no sign changes here.

use zgui_geom::{CssPx, Size};
use zgui_vocab::{ScrollDelta, ScrollPhase};

/// How many lines a browser that reports lines sends for one detent.
const LINES_PER_DETENT: f64 = 3.0;

/// The DOM delta mode for pixels.
const MODE_PIXEL: u32 = 0;

/// The DOM delta mode for lines.
const MODE_LINE: u32 = 1;

/// One wheel event, as the browser reported it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reported {
    /// The delta along each axis, in the unit of `mode`.
    pub delta: (f64, f64),
    /// The DOM delta mode.
    pub mode: u32,
    /// The legacy `wheelDeltaX` and `wheelDeltaY`. Zero where the browser reports none.
    pub legacy: (f64, f64),
    /// The height of a page in CSS pixels, for a delta in pages.
    pub page: f64,
}

/// What a wheel event asks for: detents, or a piece of a continuous stream.
///
/// A delta in lines or pages is a detent. A delta in pixels is a piece of a stream where the
/// legacy delta is three times the pixel delta, which is the ratio browsers report a touch
/// surface with, and where the browser reports no legacy delta. Every other pixel delta is a
/// detent, and it keeps the distance the browser gave it.
///
/// The ratio is the test. The size of the legacy delta cannot be one: it counts 120 for one
/// detent only on a display of ordinary density, and a scaled display changes it.
pub fn classify(event: Reported) -> (ScrollDelta, ScrollPhase) {
    let (x, y) = event.delta;
    match event.mode {
        MODE_PIXEL => {
            let pixels = ScrollDelta::Pixels(Size::new(CssPx(x as f32), CssPx(y as f32)));
            let stream = event.legacy == (0.0, 0.0)
                || is_stream(event.legacy.0, x)
                || is_stream(event.legacy.1, y);
            if stream {
                (pixels, ScrollPhase::Moved)
            } else {
                (pixels, ScrollPhase::Discrete)
            }
        }
        MODE_LINE => {
            let lines = ScrollDelta::Lines {
                x: (x / LINES_PER_DETENT) as f32,
                y: (y / LINES_PER_DETENT) as f32,
            };
            (lines, ScrollPhase::Discrete)
        }
        _ => {
            let pixels = Size::new(
                CssPx((x * event.page) as f32),
                CssPx((y * event.page) as f32),
            );
            (ScrollDelta::Pixels(pixels), ScrollPhase::Discrete)
        }
    }
}

/// Whether one axis has the ratio of a touch surface: a legacy delta of minus three times the
/// pixel delta, within a tenth.
fn is_stream(legacy: f64, pixels: f64) -> bool {
    if legacy == 0.0 || pixels == 0.0 {
        return false;
    }
    let expected = -3.0 * pixels;
    (legacy - expected).abs() <= expected.abs() * 0.1 + 1.0
}

#[cfg(test)]
mod tests {
    use super::{Reported, classify};
    use zgui_vocab::{ScrollDelta, ScrollPhase};

    fn pixels(delta: (f64, f64), legacy: (f64, f64)) -> Reported {
        Reported {
            delta,
            mode: 0,
            legacy,
            page: 800.0,
        }
    }

    fn height(delta: ScrollDelta) -> f32 {
        match delta {
            ScrollDelta::Pixels(size) => size.height.0,
            other => panic!("expected pixels, found {other:?}"),
        }
    }

    #[test]
    fn a_notched_wheel_is_a_detent_that_keeps_its_distance() {
        let (delta, phase) = classify(pixels((0.0, 100.0), (0.0, -120.0)));
        assert_eq!(height(delta), 100.0);
        assert_eq!(phase, ScrollPhase::Discrete);
    }

    #[test]
    fn a_notched_wheel_on_a_scaled_display_is_still_a_detent() {
        // What a display at scale 1.2 can report: neither delta is the round number.
        for (delta, legacy) in [(100.0, -144.0), (83.333, -120.0), (120.0, -144.0)] {
            let (_, phase) = classify(pixels((0.0, delta), (0.0, legacy)));
            assert_eq!(phase, ScrollPhase::Discrete, "{delta} with {legacy}");
        }
    }

    #[test]
    fn a_touch_surface_is_a_stream_and_keeps_its_pixels() {
        let (delta, phase) = classify(pixels((0.0, 40.0), (0.0, -120.0)));
        assert_eq!(height(delta), 40.0);
        assert_eq!(phase, ScrollPhase::Moved);

        let (_, phase) = classify(pixels((1.5, 7.25), (-4.0, -21.0)));
        assert_eq!(phase, ScrollPhase::Moved);
    }

    #[test]
    fn a_delta_in_lines_is_counted_in_detents() {
        let reported = Reported {
            delta: (0.0, -3.0),
            mode: 1,
            legacy: (0.0, 0.0),
            page: 800.0,
        };
        let (delta, phase) = classify(reported);
        assert_eq!(delta, ScrollDelta::Lines { x: 0.0, y: -1.0 });
        assert_eq!(phase, ScrollPhase::Discrete);
    }

    #[test]
    fn a_browser_with_no_legacy_delta_reports_a_stream() {
        let (_, phase) = classify(pixels((0.0, 100.0), (0.0, 0.0)));
        assert_eq!(phase, ScrollPhase::Moved);
    }
}
