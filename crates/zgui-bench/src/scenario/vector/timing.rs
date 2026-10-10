//! Paint and render time per frame, read off the latency marks the runtime takes.
//!
//! Paint is `f.paint` to `p.draw`: the emit walk, the flush and the budgets. Render is `p.draw` to
//! `f.painted`: what the renderer does with the display list. A tick that ran no frame has no
//! `f.paint` and is not sampled.
//!
//! With `ZGUI_BENCH_FRAMES` set, each rendered frame also prints one `FRAME` line: the render time,
//! then each mark from `p.draw` to `f.painted` with the time since the mark before it.

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
        let dump = std::env::var_os("ZGUI_BENCH_FRAMES").is_some();
        let mut paint = None;
        let mut draw = None;
        for (index, recorded) in marks.iter().enumerate() {
            match recorded.stage {
                "f.paint" => {
                    paint = Some(recorded.at_ns);
                    draw = None;
                }
                "p.draw" => {
                    if let Some(started) = paint.take() {
                        self.paint.push(millis(recorded.at_ns - started));
                        draw = Some(index);
                    }
                }
                "f.painted" => {
                    if let Some(from) = draw.take() {
                        self.render.push(millis(recorded.at_ns - marks[from].at_ns));
                        if dump {
                            println!("{}", frame_line(&marks[from..=index]));
                        }
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

    /// The median, the 95th percentile and the largest of paint and of render time.
    pub(super) fn spreads(&mut self) -> ((f64, f64, f64), (f64, f64, f64)) {
        (spread(&mut self.paint), spread(&mut self.render))
    }
}

/// One frame's render time and the time before each of its marks, from `p.draw` to `f.painted`.
fn frame_line(marks: &[Recorded]) -> String {
    use core::fmt::Write as _;

    let (Some(first), Some(last)) = (marks.first(), marks.last()) else {
        return String::new();
    };
    let mut line = format!("FRAME render={:.3}", millis(last.at_ns - first.at_ns));
    for pair in marks.windows(2) {
        let _ = write!(
            line,
            " {}={:.3}",
            pair[1].stage,
            millis(pair[1].at_ns - pair[0].at_ns)
        );
    }
    line
}

/// Nanoseconds as milliseconds.
#[expect(
    clippy::cast_precision_loss,
    reason = "a frame is far shorter than 2^53 nanoseconds"
)]
fn millis(nanos: u128) -> f64 {
    nanos as f64 / 1e6
}

/// The median, the 95th percentile and the largest of `samples`, or zeros when there are none.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "an index into a list of at most a few hundred samples"
)]
fn spread(samples: &mut [f64]) -> (f64, f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    samples.sort_by(f64::total_cmp);
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    (at(0.50), at(0.95), at(1.0))
}
