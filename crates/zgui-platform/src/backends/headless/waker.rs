//! A waker that records rather than interrupting anything.

use std::sync::Mutex;

use crate::app::WakeReason;
use crate::waker::Waker;

/// A waker that records rather than interrupting anything.
///
/// A headless loop has nothing to interrupt, so the wake is a queue a test drains. Recording the
/// reasons rather than only counting them is what lets a test check that work belonging to one
/// surface did not arrive as a reason to redraw every surface.
#[derive(Default)]
pub(super) struct RecordingWaker {
    pending: Mutex<Vec<WakeReason>>,
}

impl RecordingWaker {
    /// Takes everything delivered since the last call.
    pub(super) fn drain(&self) -> Vec<WakeReason> {
        std::mem::take(&mut *self.pending.lock().expect("the queue is not poisoned"))
    }
}

impl Waker for RecordingWaker {
    fn wake(&self, reason: WakeReason) {
        self.pending
            .lock()
            .expect("the queue is not poisoned")
            .push(reason);
    }
}
