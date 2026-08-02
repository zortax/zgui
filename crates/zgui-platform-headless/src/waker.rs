//! A waker that queues rather than interrupting anything.

use std::sync::Mutex;

use zgui_platform::{WakeReason, Waker};

/// A waker that queues what it was asked to deliver.
///
/// A headless loop has nothing to interrupt, so a wake is an entry in a queue the loop drains on
/// its next turn — which is what a real backend's wake amounts to as well, with the interruption
/// added. Recording the reasons rather than counting them is what lets a test check that work
/// belonging to one surface did not arrive as a reason to redraw every surface.
#[derive(Debug, Default)]
pub struct RecordingWaker {
    /// The reasons delivered and not yet drained.
    pending: Mutex<Vec<WakeReason>>,
}

impl RecordingWaker {
    /// Takes everything delivered since the last call.
    pub fn drain(&self) -> Vec<WakeReason> {
        std::mem::take(&mut *self.pending.lock().expect("the queue is not poisoned"))
    }

    /// How many wakes are waiting to be delivered.
    pub fn pending(&self) -> usize {
        self.pending
            .lock()
            .expect("the queue is not poisoned")
            .len()
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

#[cfg(test)]
mod tests {
    use super::RecordingWaker;
    use std::sync::Arc;
    use zgui_platform::{SurfaceId, WakeReason, Waker};

    #[test]
    fn a_wake_from_another_thread_is_delivered_with_the_surfaces_it_belongs_to() {
        let waker = Arc::new(RecordingWaker::default());
        let sent: Arc<dyn Waker> = Arc::clone(&waker) as Arc<dyn Waker>;
        std::thread::spawn(move || {
            sent.wake(WakeReason::ReactiveWork {
                surfaces: Box::from([SurfaceId::new(2)]),
            });
        })
        .join()
        .expect("the thread finished");

        let delivered = waker.drain();
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].surfaces(), [SurfaceId::new(2)]);
        assert_eq!(waker.pending(), 0);
    }
}
