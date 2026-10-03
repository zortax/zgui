//! The return that carries a displaced edge back to its end.
//!
//! The curve is the one AppKit and WebKit use for a rubber band: `(x₀ + A·v₀·t)·e^(−ω·t)`. With no
//! speed it is an exponential decay, so the edge starts back fast and slows as it arrives. With a
//! speed it first travels on past the end, then comes back. The curve is a function of the time
//! since the return started, so the frame rate cannot change its shape.

/// How fast the return decays, in radians per second.
///
/// AppKit's rubber-band stiffness of 20 over its period of 1.6. An edge released 100 device pixels
/// out is within 2 pixels after about 0.3 seconds.
const RATE: f32 = 12.5;

/// How much of a thrown speed becomes travel past the end.
///
/// AppKit's rubber-band amplitude. A momentum scroll at 3000 device pixels per second that reaches
/// an end travels about 27 pixels past it.
const AMPLITUDE: f32 = 0.31;

/// Below this the displacement has arrived, in device pixels.
///
/// A quarter of a device pixel is below what the device-grid snap can show, so the last step is
/// invisible. Without a floor the return never reaches zero and asks for a frame for ever.
const ARRIVED: f32 = 0.25;

/// Below this the edge is still, in device pixels per second.
const STILL: f32 = 4.0;

/// One axis of a return: where it started, the speed it started at, and how long it has run.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Return {
    /// The displacement when the return started, in device pixels.
    from: f32,
    /// The speed away from the end when the return started, in device pixels per second.
    thrown: f32,
    /// The time since the return started, in seconds.
    elapsed: f32,
}

impl Return {
    /// A return that starts at `from`, moving at `thrown`.
    pub(crate) fn start(from: f32, thrown: f32) -> Self {
        Self {
            from,
            thrown,
            elapsed: 0.0,
        }
    }

    /// The displacement now.
    pub(crate) fn at(self) -> f32 {
        (self.from + AMPLITUDE * self.thrown * self.elapsed) * (-RATE * self.elapsed).exp()
    }

    /// The speed now, in device pixels per second.
    fn speed(self) -> f32 {
        let lead = AMPLITUDE * self.thrown;
        (lead - RATE * (self.from + lead * self.elapsed)) * (-RATE * self.elapsed).exp()
    }

    /// The same return, `seconds` later.
    pub(crate) fn advanced(self, seconds: f32) -> Self {
        let advanced = Self {
            elapsed: self.elapsed + seconds,
            ..self
        };
        if advanced.arrived() {
            Self::default()
        } else {
            advanced
        }
    }

    /// The same return measured in units `by` times smaller.
    pub(crate) fn scaled(self, by: f32) -> Self {
        Self {
            from: self.from * by,
            thrown: self.thrown * by,
            elapsed: self.elapsed,
        }
    }

    /// Whether the edge is back at its end.
    ///
    /// An edge near zero on its way out has not arrived: it must also move back towards zero, or
    /// be still.
    pub(crate) fn arrived(self) -> bool {
        let at = self.at();
        let speed = self.speed();
        at.abs() < ARRIVED && (at * speed < 0.0 || speed.abs() < STILL)
    }
}

#[cfg(test)]
mod tests {
    use super::Return;

    /// One frame at sixty hertz, in seconds.
    const FRAME: f32 = 1.0 / 60.0;

    /// The displacements of a return at sixty hertz, until it arrives.
    fn run(mut edge: Return) -> Vec<f32> {
        let mut seen = vec![edge.at()];
        while !edge.arrived() && seen.len() < 600 {
            edge = edge.advanced(FRAME);
            seen.push(edge.at());
        }
        seen
    }

    #[test]
    fn a_released_edge_comes_back_in_about_half_a_second() {
        let seen = run(Return::start(100.0, 0.0));
        let frames = seen.len() - 1;
        assert!(
            (20..=40).contains(&frames),
            "the return took {frames} frames at sixty hertz"
        );
        let visible = seen.iter().position(|at| at.abs() < 2.0).expect("arrived");
        assert!(
            (15..=22).contains(&visible),
            "the edge was within two pixels after {visible} frames"
        );
    }

    #[test]
    fn a_released_edge_starts_back_fast_and_never_crosses_its_end() {
        let seen = run(Return::start(100.0, 0.0));
        assert!(seen[1] < 85.0, "the first frame moved only to {}", seen[1]);
        assert!(
            seen.windows(2)
                .all(|pair| pair[1] <= pair[0] && pair[1] >= 0.0),
            "{seen:?}"
        );
    }

    #[test]
    fn a_thrown_edge_travels_past_its_end_then_comes_back() {
        let seen = run(Return::start(0.0, 3000.0));
        let peak = seen.iter().copied().fold(0.0, f32::max);
        assert!(
            (15.0..=40.0).contains(&peak),
            "a throw at 3000 pixels per second travelled {peak}"
        );
        assert_eq!(*seen.last().expect("frames"), 0.0, "it never came back");
        assert!(seen.len() < 60, "the bounce took {} frames", seen.len());
    }

    #[test]
    fn the_frame_rate_does_not_change_where_the_edge_is() {
        let whole = Return::start(80.0, 0.0).advanced(0.048);
        let mut steps = Return::start(80.0, 0.0);
        for _ in 0..6 {
            steps = steps.advanced(0.008);
        }
        assert!((whole.at() - steps.at()).abs() < 1e-3);
    }

    #[test]
    fn a_long_frame_brings_the_edge_home_rather_than_throwing_it() {
        let after = Return::start(120.0, 0.0).advanced(2.0);
        assert_eq!(after.at(), 0.0);
        assert!(after.arrived());
    }
}
