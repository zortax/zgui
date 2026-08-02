//! The spring that carries a displaced edge back, one frame's worth at a time.

use core::time::Duration;

/// How stiff the return is, in radians per second.
///
/// A critically damped spring at this rate is visually settled in about a third of a second: fast
/// enough that the edge reads as springy rather than as slow, slow enough that the return can be
/// seen happening rather than appearing as a jump between two frames.
const RATE: f32 = 20.0;

/// Below this the displacement has arrived, in device pixels.
///
/// A quarter of a device pixel is under what the fragment pass's device-grid snap can express, so
/// the last step is invisible. Without a floor a spring never reaches zero and a container that is a
/// ten-thousandth of a pixel out is a container that asks for a frame for ever.
const ARRIVED: f32 = 0.25;

/// Below this the edge has stopped moving, in device pixels per second.
///
/// Tested *with* the displacement rather than instead of it: an edge passing through zero at speed
/// has not arrived, and one that has crept to a standstill a pixel out has not either.
const STILL: f32 = 4.0;

/// The longest step the spring is integrated over at once.
///
/// A frame that took a quarter of a second — a stall, a breakpoint, a laptop coming back from
/// sleep — integrated in one go is a spring that overshoots or diverges outright, which is a
/// container that flies off the screen for a frame. Splitting it keeps the result the same as the
/// one a smooth frame rate would have produced, which is precisely what "driven by the frame clock"
/// has to mean if it is to be honest.
const LONGEST_STEP: f32 = 1.0 / 120.0;

/// Where one axis of a displacement, and its speed, are after `elapsed`.
///
/// Critically damped: the edge returns as fast as it can without crossing zero and coming back,
/// because an overscroll that visibly bounces twice reads as a bug rather than as an edge.
pub(crate) fn advance(held: f32, speed: f32, elapsed: Duration) -> (f32, f32) {
    let mut held = held;
    let mut speed = speed;
    let mut left = elapsed.as_secs_f32();
    while left > 0.0 {
        let step = left.min(LONGEST_STEP);
        left -= step;
        // Semi-implicit Euler: the velocity is advanced first and the position with the velocity it
        // now has, which is what keeps a spring integrated in small steps from gaining energy.
        speed += (-RATE * RATE * held - 2.0 * RATE * speed) * step;
        held += speed * step;
    }
    if held.abs() < ARRIVED && speed.abs() < STILL {
        return (0.0, 0.0);
    }
    (held, speed)
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::advance;

    /// One frame at sixty hertz.
    const FRAME: Duration = Duration::from_millis(16);

    #[test]
    fn a_displaced_edge_comes_back_and_stops() {
        let (mut held, mut speed) = (100.0, 0.0);
        for _ in 0..40 {
            (held, speed) = advance(held, speed, FRAME);
        }
        assert_eq!((held, speed), (0.0, 0.0), "the edge never settled");
    }

    #[test]
    fn it_returns_within_about_a_third_of_a_second() {
        let (mut held, mut speed) = (120.0, 0.0);
        let mut frames = 0;
        while held != 0.0 && frames < 200 {
            (held, speed) = advance(held, speed, FRAME);
            frames += 1;
        }
        assert!(
            (10..=30).contains(&frames),
            "the return took {frames} frames at sixty hertz, which is not a third of a second"
        );
    }

    #[test]
    fn it_never_crosses_the_edge_and_comes_back() {
        // A spring that is not critically damped bounces, and an edge that bounces twice reads as
        // a defect rather than as an edge.
        let (mut held, mut speed) = (100.0, 0.0);
        for _ in 0..60 {
            (held, speed) = advance(held, speed, FRAME);
            assert!(held >= -0.5, "the displacement overshot to {held}");
        }
    }

    #[test]
    fn a_frame_that_took_a_quarter_of_a_second_settles_rather_than_exploding() {
        // Integrated in one step this spring diverges: the container would be thrown thousands of
        // pixels off its own edge for a frame, which a park that missed its deadline is enough to
        // produce.
        let (held, speed) = advance(120.0, 0.0, Duration::from_millis(250));
        assert!(
            held.abs() <= 120.0 && speed.abs() < 1_000.0,
            "one long frame left the edge at {held} moving at {speed}"
        );
    }

    #[test]
    fn the_same_time_in_one_step_and_in_several_reaches_the_same_place() {
        let whole = advance(80.0, 0.0, Duration::from_millis(48));
        let (mut held, mut speed) = (80.0, 0.0);
        for _ in 0..3 {
            (held, speed) = advance(held, speed, Duration::from_millis(16));
        }
        assert!(
            (whole.0 - held).abs() < 0.5,
            "a frame rate that varies would change where the edge is: {} against {held}",
            whole.0
        );
    }
}
