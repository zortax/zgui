//! Noticing that the device died.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// The flag a device-lost callback sets, and the counter of validation failures beside it.
///
/// The callback runs on whatever thread the driver chooses, so it may not touch the renderer. All
/// it does is record the fact; the next frame reads it and rebuilds. That ordering is why recovery
/// is never half-applied: nothing is torn down inside a driver callback.
#[derive(Debug, Default)]
pub struct DeviceLoss {
    /// Whether the device has been reported lost.
    lost: AtomicBool,
    /// How many times acquisition has failed validation in a row.
    consecutive_validation_failures: AtomicU32,
}

impl DeviceLoss {
    /// How many validation failures in a row are treated as a lost device.
    ///
    /// One is a bug to log; ten in a row is a device that will not recover on its own, and the
    /// same bounded-retry rule the instance buffers use applies: retry, but never forever.
    pub const VALIDATION_FAILURE_LIMIT: u32 = 10;

    /// A device that has not been lost.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that the device was lost, with the driver's reason.
    pub fn report(&self, reason: wgpu::DeviceLostReason, message: &str) {
        self.lost.store(true, Ordering::Release);
        tracing::error!(?reason, message, "the graphics device was lost");
    }

    /// Whether the device has been reported lost.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    /// Records one acquisition validation failure, and says whether it is the one that escalates.
    ///
    /// Returns `true` once the run reaches [`DeviceLoss::VALIDATION_FAILURE_LIMIT`], which is the
    /// caller's cue to rebuild the device rather than to keep asking.
    pub fn note_validation_failure(&self) -> bool {
        let seen = self
            .consecutive_validation_failures
            .fetch_add(1, Ordering::AcqRel)
            + 1;
        seen >= Self::VALIDATION_FAILURE_LIMIT
    }

    /// Records that acquisition succeeded, ending any run of failures.
    pub fn note_acquisition_succeeded(&self) {
        self.consecutive_validation_failures
            .store(0, Ordering::Release);
    }

    /// How many validation failures have happened in a row.
    pub fn consecutive_validation_failures(&self) -> u32 {
        self.consecutive_validation_failures.load(Ordering::Acquire)
    }

    /// Forgets the loss, once the device behind it has been rebuilt.
    pub fn clear(&self) {
        self.lost.store(false, Ordering::Release);
        self.consecutive_validation_failures
            .store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::DeviceLoss;

    #[test]
    fn a_run_of_validation_failures_escalates_exactly_at_the_limit() {
        let loss = DeviceLoss::new();
        for failure in 1..DeviceLoss::VALIDATION_FAILURE_LIMIT {
            assert!(
                !loss.note_validation_failure(),
                "failure {failure} escalated early"
            );
        }
        assert!(
            loss.note_validation_failure(),
            "the tenth consecutive failure must escalate"
        );
    }

    #[test]
    fn one_success_ends_the_run() {
        let loss = DeviceLoss::new();
        for _ in 0..5 {
            assert!(!loss.note_validation_failure());
        }
        loss.note_acquisition_succeeded();
        assert_eq!(loss.consecutive_validation_failures(), 0);
        assert!(!loss.note_validation_failure());
    }

    #[test]
    fn a_lost_device_stays_lost_until_it_is_rebuilt() {
        let loss = DeviceLoss::new();
        assert!(!loss.is_lost());
        loss.report(wgpu::DeviceLostReason::Unknown, "test");
        assert!(loss.is_lost());
        loss.clear();
        assert!(!loss.is_lost());
    }
}
