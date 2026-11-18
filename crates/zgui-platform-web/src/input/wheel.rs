//! Wheel events as detents or as a continuous stream.
//!
//! A browser reports a notched wheel and a touch surface through one event, and mostly in the same
//! unit. The two need opposite treatment: a detent is one whole step that the framework carries
//! over several frames, and a touch surface is a stream the system already animates, momentum
//! included. [`classify`] tells them apart.
//!
//! A positive delta moves the scroll offset right and down, in the browser and in the framework,
//! so no sign changes here.

use zgui_geom::{CssPx, Size};
use zgui_vocab::{ScrollDelta, ScrollPhase};

/// The legacy wheel delta of one detent.
const DETENT: f64 = 120.0;

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
    /// The legacy `wheelDeltaX` and `wheelDeltaY`, which count 120 for one detent. Zero where the
    /// browser reports none.
    pub legacy: (f64, f64),
    /// The height of a page in CSS pixels, for a delta in pages.
    pub page: f64,
}

/// What a wheel event asks for: detents, or a piece of a continuous stream.
///
/// A delta in lines or pages is a detent. A delta in pixels is a detent where the legacy delta is
/// a whole number of detents and is not three times the pixel delta, which is the ratio a touch
/// surface reports with. Every other pixel delta is a piece of a stream.
pub fn classify(event: Reported) -> (ScrollDelta, ScrollPhase) {
    let (x, y) = event.delta;
    match event.mode {
        MODE_PIXEL => {
            if is_detent(event.legacy.0, x)
                && is_detent(event.legacy.1, y)
                && event.legacy != (0.0, 0.0)
            {
                let lines = ScrollDelta::Lines {
                    x: (-event.legacy.0 / DETENT) as f32,
                    y: (-event.legacy.1 / DETENT) as f32,
                };
                (lines, ScrollPhase::Discrete)
            } else {
                let pixels = Size::new(CssPx(x as f32), CssPx(y as f32));
                (ScrollDelta::Pixels(pixels), ScrollPhase::Moved)
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

/// Whether one axis of a pixel delta is a whole number of detents, or is at rest.
fn is_detent(legacy: f64, pixels: f64) -> bool {
    if legacy == 0.0 {
        return pixels == 0.0;
    }
    legacy % DETENT == 0.0 && legacy != -3.0 * pixels
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

    #[test]
    fn a_notched_wheel_in_pixels_is_one_detent_down() {
        let (delta, phase) = classify(pixels((0.0, 100.0), (0.0, -120.0)));
        assert_eq!(delta, ScrollDelta::Lines { x: 0.0, y: 1.0 });
        assert_eq!(phase, ScrollPhase::Discrete);
    }

    #[test]
    fn a_touch_surface_is_a_stream_and_keeps_its_pixels() {
        let (delta, phase) = classify(pixels((0.0, 40.0), (0.0, -120.0)));
        assert!(matches!(delta, ScrollDelta::Pixels(size) if size.height.0 == 40.0));
        assert_eq!(phase, ScrollPhase::Moved);

        let (delta, _) = classify(pixels((1.5, 7.25), (-4.0, -21.0)));
        assert!(matches!(delta, ScrollDelta::Pixels(_)));
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
        let (delta, phase) = classify(pixels((0.0, 100.0), (0.0, 0.0)));
        assert!(matches!(delta, ScrollDelta::Pixels(_)));
        assert_eq!(phase, ScrollPhase::Moved);
    }
}
