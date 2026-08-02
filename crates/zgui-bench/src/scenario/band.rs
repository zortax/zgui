//! What a number is allowed to be, and what it means when it is not.
//!
//! A measurement with no band around it is a number in a log: it can double without anybody
//! noticing, because nothing compares it to anything. A band is the comparison, written down once
//! and applied by the harness that takes the measurement, so a regression is a non-zero exit rather
//! than a line somebody has to read.
//!
//! Two shapes, because the numbers are of two kinds.
//!
//! A **time** is a property of the machine as much as of the code. It moves with the core the
//! scheduler picked, with what else is running, and with the state of the caches, so its band is a
//! multiple of a recorded baseline rather than a fixed number, and the multiple has to clear the
//! ordinary spread of the measurement on an ordinary machine.
//!
//! A **count** is a property of the design. "One row hovered, one row restyled" is true on a slow
//! machine, a fast one and under a debugger, so its band is a ceiling with no tolerance at all: the
//! moment a frame emits a two-hundredth primitive, something stopped culling.

use core::fmt;

/// What a measurement is allowed to be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Band {
    /// A duration, in microseconds, that may exceed `baseline` by `tolerance` of itself.
    ///
    /// `tolerance` is a fraction: `0.4` admits a value forty per cent above the baseline.
    Time {
        /// What the measurement was when the band was written.
        baseline: f64,
        /// How far above it a run may land before it counts as a regression.
        tolerance: f64,
    },
    /// A count that may not exceed `ceiling`, with no tolerance.
    Count {
        /// The largest value that is not a regression.
        ceiling: u64,
    },
}

impl Band {
    /// The largest value inside the band.
    pub(crate) fn limit(self) -> f64 {
        match self {
            Self::Time {
                baseline,
                tolerance,
            } => baseline * (1.0 + tolerance),
            #[expect(
                clippy::cast_precision_loss,
                reason = "a counter ceiling is a small integer, and the comparison it feeds is \
                          against a value that came from the same integer domain"
            )]
            Self::Count { ceiling } => ceiling as f64,
        }
    }

    /// Whether `value` is inside the band.
    pub(crate) fn admits(self, value: f64) -> bool {
        value <= self.limit()
    }

    /// How the band reads in a report.
    pub(crate) fn describe(self) -> String {
        match self {
            Self::Time {
                baseline,
                tolerance,
            } => format!(
                "{baseline:.1} +{:.0}% = {:.1}",
                tolerance * 100.0,
                self.limit()
            ),
            Self::Count { ceiling } => format!("<= {ceiling}"),
        }
    }
}

/// The shape of a set of samples, rather than its middle.
///
/// A median is what a run felt like on its good frames. It is silent about the frame that took
/// twenty milliseconds, and a stall every thirtieth frame is exactly what a person calls "not
/// smooth" — so a duration reported as one number is a duration reported as the wrong number. Four
/// quantiles and the sample count is the smallest honest answer: the middle, the shoulder, the tail
/// and the worst thing that happened, over a population whose size is stated so that a `p99` taken
/// from eleven samples cannot pass itself off as a tail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Spread {
    /// The middle.
    pub(crate) p50: f64,
    /// The shoulder.
    pub(crate) p95: f64,
    /// The tail.
    pub(crate) p99: f64,
    /// The worst sample taken.
    pub(crate) max: f64,
    /// How many samples the four figures were taken from.
    pub(crate) samples: usize,
}

impl Spread {
    /// The spread of `samples`, which are sorted in place.
    ///
    /// # Panics
    ///
    /// Panics on an empty slice. A distribution over nothing is not a weaker measurement, it is
    /// four zeroes that read as a fast run.
    pub(crate) fn of(samples: &mut [f64]) -> Self {
        assert!(
            !samples.is_empty(),
            "a distribution was asked for over no samples at all"
        );
        samples.sort_by(f64::total_cmp);
        Self {
            p50: at(samples, 0.50),
            p95: at(samples, 0.95),
            p99: at(samples, 0.99),
            max: samples[samples.len() - 1],
            samples: samples.len(),
        }
    }
}

/// The value below which `fraction` of a sorted slice sits.
fn at(sorted: &[f64], fraction: f64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "sample counts here are in the hundreds and the index is clamped into the slice"
    )]
    let index = (((sorted.len() - 1) as f64) * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// How a scenario's frames landed against the interval it was pacing at.
///
/// The other half of the distribution, and the half that says whether a person would have seen the
/// difference. A frame that cost three milliseconds and arrived after its interval had already
/// elapsed is a frame nobody saw on time, and no quantile of the frame *cost* can say so — the cost
/// was fine. What is wanted is frames delivered, so what is counted is intervals missed, against a
/// stated refresh, over a stated population.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pace {
    /// The interval the frames were paced against, in microseconds.
    pub(crate) interval_us: f64,
    /// How many frames took longer than [`Pace::LATE`] times that interval.
    pub(crate) late: usize,
    /// How many frames were measured.
    pub(crate) frames: usize,
}

impl Pace {
    /// How far past its interval a frame goes before it is late.
    ///
    /// One and a half, matching the fraction the pacing criteria are stated in: an interval that
    /// slipped by half of itself is one a display had to repeat, and a repeated frame is what a
    /// person sees as a hitch.
    pub(crate) const LATE: f64 = 1.5;

    /// How `samples` landed against an interval of `interval_us` microseconds.
    pub(crate) fn of(samples: &[f64], interval_us: f64) -> Self {
        let ceiling = interval_us * Self::LATE;
        Self {
            interval_us,
            late: samples.iter().filter(|cost| **cost > ceiling).count(),
            frames: samples.len(),
        }
    }

    /// What fraction of the frames were late.
    #[expect(
        clippy::cast_precision_loss,
        reason = "frame counts here are in the hundreds"
    )]
    pub(crate) fn late_fraction(&self) -> f64 {
        if self.frames == 0 {
            return 0.0;
        }
        self.late as f64 / self.frames as f64
    }
}

/// One thing a scenario measures, with the band it has to stay inside.
#[derive(Clone, Debug)]
pub(crate) struct Measurement {
    /// What was measured, as a report names it.
    pub(crate) name: &'static str,
    /// What it is counted in — `us`, `ms`, or the name of the thing counted.
    pub(crate) unit: &'static str,
    /// What this run measured.
    pub(crate) value: f64,
    /// What it is allowed to be.
    pub(crate) band: Band,
    /// Why the band is this wide, in one sentence, so a reader never has to guess.
    pub(crate) rationale: &'static str,
    /// The budget the schedule wrote for this quantity, where it wrote one.
    ///
    /// A budget and a band are different instruments and both are worth having. The band says
    /// *this build got slower*, and it is the gate: it is written against what was measured, so it
    /// fires on a change and stays quiet on a machine that is merely slower than the one the
    /// numbers were taken on. The budget says *this is what the design was supposed to cost*, and
    /// it does not move with the measurement — which means a number can sit inside its band for
    /// ever while never once having met the budget. Reporting both is the only way to tell those
    /// two states apart, and a budget that is missed is what opens an escalation rather than what
    /// fails a run.
    pub(crate) budget: Option<f64>,
    /// The distribution [`Measurement::value`] was taken from, where the value is a duration.
    ///
    /// Required of every duration and refused to every count: a count is one number about a
    /// design and has no tail, while a duration without one is a claim about smoothness made from
    /// the half of the evidence that cannot contradict it. `tails` is the gate that enforces both
    /// halves of that rule.
    pub(crate) spread: Option<Spread>,
}

impl Measurement {
    /// Whether this run stayed inside the band.
    pub(crate) fn passed(&self) -> bool {
        self.band.admits(self.value)
    }

    /// Whether this run met the schedule's budget, where there is one.
    pub(crate) fn met_budget(&self) -> Option<bool> {
        self.budget.map(|budget| self.value <= budget)
    }
}

impl fmt::Display for Measurement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:<34} {:>12.2} {:<4} band {:<24} {}",
            self.name,
            self.value,
            self.unit,
            self.band.describe(),
            if self.passed() { "ok" } else { "REGRESSED" },
        )
    }
}

/// The tolerance a microsecond-scale interaction gets.
///
/// The scenarios take their times from a headless run on a virtual clock, so nothing here waits on
/// a display; what is left in the measurement is CPU work and whatever the machine was doing at the
/// time. Repeat to repeat on a quiet machine the median moves by a few per cent, and process to
/// process on a machine that is also building something it moves by tens. Forty per cent clears the
/// second of those and is far under the smallest regression worth catching: the fast paths these
/// interactions ride on do not get ten per cent worse when one is lost, they get several times
/// worse, because a lost fast path means a whole document restyled, relaid out or re-emitted.
pub(crate) const INTERACTION_TOLERANCE: f64 = 0.40;

/// The tolerance a millisecond-scale measurement gets.
///
/// A number in milliseconds carries hundreds of times as much work as one in microseconds, so the
/// per-run noise is proportionally smaller: the cold-start figure this band was written against
/// spread over 216–247 ms across runs, which is seven per cent either side of its middle. Twenty-
/// five per cent sits above that spread with room to spare and still catches the kind of change
/// that matters here — a font enumeration that stopped being cached, a sheet parsed twice.
pub(crate) const STARTUP_TOLERANCE: f64 = 0.25;

/// The two counts that say a change to one element was answered by invalidating the document.
///
/// A shaped paragraph carries the brush slot its glyphs were shaped with, so an element that has to
/// leave a slot it was sharing has to be shaped again. That is one element's obligation, and the
/// two numbers below are what a window reports when it discharges it by throwing away **every**
/// shaped paragraph in the window and invalidating **every** box in the layout tree: one focus
/// move, one hover, one frame of a scroll animation, and the whole document is measured and
/// arranged from nothing.
///
/// The ceiling is zero and it has no tolerance, because this is a property of the design rather
/// than of the machine: an interaction that changes one element either reaches for the
/// document-wide invalidation or it does not, and the answer is the same under a debugger as it is
/// on the fastest machine anybody owns. Nothing a scenario here drives is entitled to either — the
/// two callers that legitimately are, a change of device scale and a shaping cache over its budget,
/// are not what any of these scenarios do.
///
/// Both halves are reported rather than one, because either alone is still a whole-document cost:
/// the paragraphs are what has to be shaped again, and the boxes are what has to be measured and
/// arranged again from them, and a fix that scoped one while leaving the other would read as green.
pub(crate) fn whole_document_reshape(moved: &zgui_profile::Counters) -> [Measurement; 2] {
    [
        Measurement {
            name: "reshape.paragraphs_forgotten",
            unit: "paragraphs",
            value: count(moved.paragraphs_forgotten),
            band: Band::Count { ceiling: 0 },
            rationale: "one element's brush moving costs that element's paragraphs, not the \
                        window's",
            budget: None,
            spread: None,
        },
        Measurement {
            name: "reshape.boxes_marked_all_dirty",
            unit: "boxes",
            value: count(moved.boxes_marked_all_dirty),
            band: Band::Count { ceiling: 0 },
            rationale: "nothing these scenarios drive changes the device scale, which is the one \
                        change every box pays for",
            budget: None,
            spread: None,
        },
    ]
}

/// A counter as a measured value.
#[expect(
    clippy::cast_precision_loss,
    reason = "a counter compared against a ceiling of zero, where every value that matters is \
              small and every value that is not is a regression either way"
)]
fn count(value: u64) -> f64 {
    value as f64
}
