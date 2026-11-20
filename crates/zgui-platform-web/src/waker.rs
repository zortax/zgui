//! The way into the loop from outside a turn.

use std::collections::VecDeque;
use std::sync::Mutex;

use zgui_platform::{WakeReason, Waker};

/// Holds the reasons the application was woken for until the next turn reads them.
#[derive(Debug, Default)]
pub(crate) struct PageWaker {
    /// The reasons not yet delivered, oldest first.
    pending: Mutex<VecDeque<WakeReason>>,
}

impl PageWaker {
    /// Takes every reason that has arrived.
    pub(crate) fn take(&self) -> VecDeque<WakeReason> {
        std::mem::take(
            &mut *self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

impl Waker for PageWaker {
    fn wake(&self, reason: WakeReason) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(reason);
        // A turn of its own, after the current callback returns: a wake that ran inside the turn
        // that caused it could keep the page from painting.
        crate::driver::schedule_turn();
    }
}
