//! How another thread asks the loop to wake up.

use crate::app::WakeReason;

/// A handle that wakes the loop from anywhere.
///
/// This is the only object in the platform contract that crosses threads, and it exists because
/// almost everything interesting happens on another one: a file finishing loading, an image
/// finishing its decode, an assistive technology asking for something on its own connection. Each
/// of those has to reach a loop that is parked, and none of them may touch anything the loop owns.
///
/// So the handle is shareable and sendable, it carries a reason and nothing else, and the loop
/// does the work. A backend implements it over whatever its event loop offers for the purpose.
pub trait Waker: Send + Sync + 'static {
    /// Wakes the loop, which will deliver `reason` on its own thread.
    ///
    /// This never blocks and never fails. A wake sent to a loop that has already finished is
    /// discarded, because the alternative — an error every caller has to handle at shutdown —
    /// would be handled by ignoring it anyway.
    fn wake(&self, reason: WakeReason);
}

#[cfg(test)]
mod tests {
    use super::Waker;
    use crate::app::WakeReason;
    use std::sync::{Arc, Mutex};

    /// A waker that records what it was asked to deliver.
    #[derive(Default)]
    struct Recording {
        reasons: Mutex<Vec<String>>,
    }

    impl Waker for Recording {
        fn wake(&self, reason: WakeReason) {
            self.reasons
                .lock()
                .expect("the record is not poisoned")
                .push(format!("{reason:?}"));
        }
    }

    #[test]
    fn a_waker_is_shareable_across_threads() {
        let waker: Arc<dyn Waker> = Arc::new(Recording::default());
        let sent = Arc::clone(&waker);
        std::thread::spawn(move || sent.wake(WakeReason::DeviceLost))
            .join()
            .expect("the thread finished");
    }

    #[test]
    fn a_waker_carries_its_reason_through() {
        let recording = Arc::new(Recording::default());
        let waker: Arc<dyn Waker> = Arc::clone(&recording) as Arc<dyn Waker>;
        waker.wake(WakeReason::ColorSchemeChanged);
        let reasons = recording
            .reasons
            .lock()
            .expect("the record is not poisoned");
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].contains("ColorSchemeChanged"));
    }
}
