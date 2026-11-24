//! The velocity of a finger, estimated from the steps it moves by.

use core::time::Duration;

use zgui_geom::{Device, DevicePx, Size};

use crate::chain;

// The estimate is the one Android, Chromium and Flutter use for touch: a quadratic least-squares
// fit of the position over at most 20 samples of the last 100 ms, and a finger that sends nothing
// for 40 ms has stopped. Steps at 576 Hz fill the 20 samples in 35 ms. GTK's kinetic scrolling divides the motion of the last 150 ms by its
// span instead, which reads the average speed over that span. For a finger that slows down, that
// is faster than the speed at lift-off: about 27 % for a speed that decays with a 325 ms time
// constant.

/// How far back the estimate reaches from the newest step.
const HORIZON: Duration = Duration::from_millis(100);

/// A gap between steps longer than this means the finger stopped.
const STOPPED: Duration = Duration::from_millis(40);

/// Steps closer together than this are merged into one sample.
const COALESCED: Duration = Duration::from_millis(1);

/// How many samples the estimate fits at most.
const SAMPLES: usize = 20;

/// Where the finger was at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Sample {
    /// The stamp of the step, as time since an arbitrary origin.
    at: Duration,
    /// How far the finger had moved since the gesture began, in device pixels.
    position: [f64; 2],
}

/// An estimate of a finger's velocity from the steps it moved by and their stamps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VelocityTracker {
    /// The samples, oldest first.
    samples: [Sample; SAMPLES],
    /// How many of `samples` are recorded.
    len: usize,
    /// How far the finger has moved since the gesture began, in device pixels.
    position: [f64; 2],
    /// The stamp of the latest step, moving or not.
    latest: Option<Duration>,
}

impl VelocityTracker {
    /// A tracker that has seen no step.
    pub(crate) fn new() -> Self {
        Self {
            samples: [Sample::default(); SAMPLES],
            len: 0,
            position: [0.0; 2],
            latest: None,
        }
    }

    /// Records that the finger moved by `delta` in a step stamped `at`.
    pub(crate) fn moved(&mut self, delta: Size<DevicePx, Device>, at: Duration) {
        self.latest = Some(self.latest.map_or(at, |latest| latest.max(at)));
        if chain::negligible(delta) {
            return;
        }
        self.position[0] += f64::from(delta.width.0);
        self.position[1] += f64::from(delta.height.0);
        let sample = Sample {
            at,
            position: self.position,
        };

        match self.newest() {
            Some(newest) if at.saturating_sub(newest.at) > STOPPED => {
                self.samples[0] = sample;
                self.len = 1;
            }
            // Steps a platform delivers in one batch arrive with one stamp.
            Some(newest) if at.saturating_sub(newest.at) < COALESCED => {
                self.samples[self.len - 1] = Sample {
                    at: newest.at.max(at),
                    position: self.position,
                };
            }
            _ if self.len == SAMPLES => {
                self.samples.copy_within(1.., 0);
                self.samples[SAMPLES - 1] = sample;
            }
            _ => {
                self.samples[self.len] = sample;
                self.len += 1;
            }
        }
    }

    /// The velocity of the finger, in device pixels per second.
    ///
    /// The velocity is the one at the newest step that moved, fitted to at most the last 20 steps
    /// of the 100 ms before it, so it follows a finger that slows down. Steps that move less than a
    /// hundredth of a device pixel, such as the one that ends a gesture, are left out of the fit. The velocity
    /// is zero if the finger never moved, or rested for more than 40 ms before the latest step.
    pub(crate) fn velocity(&self) -> Size<DevicePx, Device> {
        let still = Size::new(DevicePx(0.0), DevicePx(0.0));
        let (Some(newest), Some(latest)) = (self.newest(), self.latest) else {
            return still;
        };
        if latest.saturating_sub(newest.at) > STOPPED {
            return still;
        }

        let recent = &self.samples[..self.len];
        let first = recent
            .iter()
            .position(|sample| newest.at.saturating_sub(sample.at) <= HORIZON)
            .unwrap_or(self.len - 1);
        let fit = |axis: usize| {
            let points = recent[first..].iter().map(|sample| {
                let t = (sample.at.as_secs_f64() - newest.at.as_secs_f64()) * 1000.0;
                (t, sample.position[axis] - newest.position[axis])
            });
            slope(points) as f32
        };
        Size::new(DevicePx(fit(0)), DevicePx(fit(1)))
    }

    fn newest(&self) -> Option<Sample> {
        self.len.checked_sub(1).map(|last| self.samples[last])
    }
}

/// Returns the slope at `t = 0` of the least-squares fit of `points`, given as `(t, x)` with `t`
/// in milliseconds, in units of `x` per second.
///
/// The fit is quadratic for three points or more, and a straight line for two. A single point
/// gives zero.
fn slope(points: impl Iterator<Item = (f64, f64)>) -> f64 {
    // The sums of t^k for k in 0..5, and of x * t^k for k in 0..3.
    let mut t = [0.0; 5];
    let mut xt = [0.0; 3];
    for (at, x) in points {
        let mut power = 1.0;
        for (k, sum) in t.iter_mut().enumerate() {
            *sum += power;
            if let Some(sum) = xt.get_mut(k) {
                *sum += x * power;
            }
            power *= at;
        }
    }

    let quadratic = [[t[0], t[1], t[2]], [t[1], t[2], t[3]], [t[2], t[3], t[4]]];
    let determinant = det(quadratic);
    if t[0] >= 3.0 && determinant.abs() > f64::EPSILON {
        let mut with_x = quadratic;
        for (row, x) in with_x.iter_mut().zip(xt) {
            row[1] = x;
        }
        return det(with_x) / determinant * 1000.0;
    }
    let line = t[0] * t[2] - t[1] * t[1];
    if t[0] >= 2.0 && line.abs() > f64::EPSILON {
        return (t[0] * xt[1] - t[1] * xt[0]) / line * 1000.0;
    }
    0.0
}

/// The determinant of `m`.
fn det(m: [[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{DevicePx, Size};

    use super::VelocityTracker;

    fn by(x: f32, y: f32) -> Size<DevicePx, zgui_geom::Device> {
        Size::new(DevicePx(x), DevicePx(y))
    }

    #[test]
    fn the_velocity_is_measured_from_the_stamps() {
        let mut tracker = VelocityTracker::new();
        for step in 0..10 {
            tracker.moved(by(0.0, 10.0), Duration::from_millis(10 * step));
        }
        let velocity = tracker.velocity();
        assert!((velocity.height.0 - 1000.0).abs() < 1.0, "{velocity:?}");
    }
}
