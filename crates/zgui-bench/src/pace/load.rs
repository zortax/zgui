//! What else the machine was doing, without which a pacing number is not a pacing number.

/// The machine's load while a measurement was taken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Load {
    /// The one-minute run-queue average when the run started.
    pub(crate) before: f64,
    /// The one-minute run-queue average when it ended.
    pub(crate) after: f64,
}

impl Load {
    /// Reads the average now.
    ///
    /// Returns `None` where the system does not publish one, which is the case a caller has to
    /// handle by refusing to publish rather than by assuming the machine was quiet.
    pub(crate) fn now() -> Option<f64> {
        let text = std::fs::read_to_string("/proc/loadavg").ok()?;
        text.split_whitespace().next()?.parse().ok()
    }

    /// The highest of the two samples, which is the figure a reader should judge the run by.
    pub(crate) fn peak(self) -> f64 {
        self.before.max(self.after)
    }

    /// How the load reads in a report.
    pub(crate) fn describe(self) -> String {
        format!(
            "load1 {:.2} at the start and {:.2} at the end",
            self.before, self.after
        )
    }
}

/// What a run may not publish through.
///
/// A pacing figure taken while a compilation was running is a figure about the compilation. There
/// is no honest way to correct for it after the fact, so the run refuses instead — and the ceiling
/// is stated rather than guessed at: one runnable task besides this one is an ordinary desktop, and
/// four is a machine with something else on it.
pub(crate) const BUSY: f64 = 4.0;
