//! How another thread reaches a loop that is asleep on the compositor's socket.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use zgui_platform::{WakeReason, Waker};

/// The queue a wake is left in, and the signal that gets the loop to read it.
///
/// The two halves are separate because they cross the thread boundary differently. A reason is a
/// value that has to be moved to the loop's thread, so it is left under a lock. The interruption
/// itself is a byte down a pipe, which is the only thing that can reach a thread blocked in a poll
/// and the only thing that is safe to do from a signal-adjacent context.
///
/// Reasons are kept in arrival order. Two pieces of background work finishing in one order and
/// being reported in the other would be a scheduling difference nothing above could see or
/// reproduce.
#[derive(Debug)]
pub struct PingWaker {
    /// The reasons waiting to be delivered.
    queued: Mutex<VecDeque<WakeReason>>,
    /// The interruption.
    ping: calloop::ping::Ping,
}

impl PingWaker {
    /// A waker that leaves reasons here and interrupts the loop through `ping`.
    pub fn new(ping: calloop::ping::Ping) -> Arc<Self> {
        Arc::new(Self {
            queued: Mutex::new(VecDeque::new()),
            ping,
        })
    }

    /// Everything left since the last drain, in arrival order.
    ///
    /// Drained as a whole rather than one at a time: the loop delivers a batch per turn, and
    /// taking them one at a time would let a wake arriving mid-delivery be answered before one
    /// that was already waiting.
    pub fn drain(&self) -> VecDeque<WakeReason> {
        let mut queued = self.lock();
        std::mem::take(&mut *queued)
    }

    /// The signal itself, for a surface that has to interrupt the loop from another thread.
    pub fn ping(&self) -> calloop::ping::Ping {
        self.ping.clone()
    }

    /// Whether anything is waiting.
    pub fn is_pending(&self) -> bool {
        !self.lock().is_empty()
    }

    /// The queue, recovering from a panic on another thread.
    ///
    /// A poisoned lock here means a thread panicked while queueing a wake. The queue itself is a
    /// list of plain values and cannot be left half-written, and refusing to wake the loop for
    /// ever afterwards turns one thread's panic into a frozen application.
    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<WakeReason>> {
        self.queued
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Waker for PingWaker {
    fn wake(&self, reason: WakeReason) {
        self.lock().push_back(reason);
        self.ping.ping();
    }
}

#[cfg(test)]
mod tests {
    use super::PingWaker;
    use zgui_platform::{SurfaceId, WakeReason, Waker};

    fn waker() -> (std::sync::Arc<PingWaker>, calloop::ping::PingSource) {
        let (ping, source) = calloop::ping::make_ping().expect("a pipe can be made");
        (PingWaker::new(ping), source)
    }

    #[test]
    fn nothing_is_waiting_on_a_fresh_waker() {
        let (waker, _source) = waker();
        assert!(!waker.is_pending());
        assert!(waker.drain().is_empty());
    }

    #[test]
    fn reasons_come_back_in_the_order_they_arrived() {
        let (waker, _source) = waker();
        waker.wake(WakeReason::ReactiveWork {
            surfaces: Box::from([SurfaceId::new(1)]),
        });
        waker.wake(WakeReason::AppWork);
        waker.wake(WakeReason::DeviceLost);

        let drained: Vec<WakeReason> = waker.drain().into();
        assert_eq!(drained.len(), 3);
        assert_eq!(drained[0].surfaces(), [SurfaceId::new(1)]);
        assert!(matches!(drained[1], WakeReason::AppWork));
        assert!(matches!(drained[2], WakeReason::DeviceLost));
    }

    #[test]
    fn draining_leaves_the_queue_empty_for_the_next_turn() {
        let (waker, _source) = waker();
        waker.wake(WakeReason::AppWork);
        assert!(waker.is_pending());
        assert_eq!(waker.drain().len(), 1);
        assert!(!waker.is_pending());
        assert!(waker.drain().is_empty());
    }

    #[test]
    fn a_waker_can_be_used_from_another_thread() {
        let (waker, _source) = waker();
        let sender = std::sync::Arc::clone(&waker);
        std::thread::spawn(move || sender.wake(WakeReason::AppWork))
            .join()
            .expect("the sending thread finished");
        assert_eq!(waker.drain().len(), 1);
    }
}
