//! How long before a driver's latch a window starts the frame the latch is to take.

use web_time::Duration;

/// How many recent frames the lead looks back over.
const FRAMES: usize = 16;

/// What the lead adds to the frames' cost, for a wake that comes late.
const MARGIN: Duration = Duration::from_millis(1);

/// The lead a window keeps before its driver's latch, measured from its recent frames.
///
/// The lead is the second slowest recent frame's time from its start to its end, plus a
/// millisecond. One slow frame, such as the first after a launch, does not set it. Before any frame
/// is measured it is the margin alone.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ComposeLead {
    costs: [Duration; FRAMES],
    next: usize,
    measured: usize,
}

impl ComposeLead {
    /// Records that a frame took `cost` from its start to its end.
    pub(crate) fn record(&mut self, cost: Duration) {
        self.costs[self.next] = cost;
        self.next = (self.next + 1) % FRAMES;
        self.measured = (self.measured + 1).min(FRAMES);
    }

    /// The lead to keep before a latch.
    pub(crate) fn get(&self) -> Duration {
        let mut costs = self.costs;
        let recent = &mut costs[..self.measured];
        recent.sort_unstable();
        let cost = recent
            .len()
            .checked_sub(2)
            .or_else(|| recent.len().checked_sub(1))
            .map_or(Duration::ZERO, |index| recent[index]);
        cost + MARGIN
    }
}

#[cfg(test)]
mod tests {
    use super::{ComposeLead, MARGIN};
    use web_time::Duration;

    #[test]
    fn one_slow_frame_does_not_set_the_lead() {
        let mut lead = ComposeLead::default();
        assert_eq!(lead.get(), MARGIN);
        lead.record(Duration::from_millis(30));
        for _ in 0..4 {
            lead.record(Duration::from_micros(400));
        }
        assert_eq!(lead.get(), Duration::from_micros(400) + MARGIN);
    }
}
