//! Paint and render time per frame, read off the latency marks the runtime takes.
//!
//! Paint is `f.paint` to `p.draw`: the emit walk, the flush and the budgets. Render is `p.draw` to
//! `f.painted`: what the renderer does with the display list. A tick that ran no frame has no
//! `f.paint` and is not sampled.

use zgui_profile::latency::Recorded;

/// Starts keeping the marks in memory, notes left out.
pub(super) fn start() {
    zgui_profile::latency::retain_marks(8192);
}

/// The paint and render times of every frame a stretch ran, in milliseconds.
#[derive(Default)]
pub(super) struct Frames {
    /// Paint time per frame.
    paint: Vec<f64>,
    /// Render time per frame.
    render: Vec<f64>,
}

impl Frames {
    /// Adds every complete frame in `marks`.
    pub(super) fn read(&mut self, marks: &[Recorded]) {
        let mut paint = None;
        let mut draw = None;
        for recorded in marks {
            match recorded.stage {
                "f.paint" => {
                    paint = Some(recorded.at_ns);
                    draw = None;
                }
                "p.draw" => {
                    if let Some(started) = paint.take() {
                        self.paint.push(millis(recorded.at_ns - started));
                        draw = Some(recorded.at_ns);
                    }
                }
                "f.painted" => {
                    if let Some(started) = draw.take() {
                        self.render.push(millis(recorded.at_ns - started));
                    }
                    paint = None;
                }
                _ => {}
            }
        }
    }

    /// How many frames were sampled.
    pub(super) fn len(&self) -> usize {
        self.paint.len()
    }

    /// The median and the 95th percentile of paint and of render time.
    pub(super) fn spreads(&mut self) -> ((f64, f64), (f64, f64)) {
        (spread(&mut self.paint), spread(&mut self.render))
    }
}

/// Nanoseconds as milliseconds.
#[expect(
    clippy::cast_precision_loss,
    reason = "a frame is far shorter than 2^53 nanoseconds"
)]
fn millis(nanos: u128) -> f64 {
    nanos as f64 / 1e6
}

/// The median and the 95th percentile of `samples`, or zeros when there are none.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "an index into a list of at most a few hundred samples"
)]
fn spread(samples: &mut [f64]) -> (f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    samples.sort_by(f64::total_cmp);
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    (at(0.50), at(0.95))
}
