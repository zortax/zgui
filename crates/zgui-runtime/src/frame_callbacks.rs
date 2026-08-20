//! Frame callbacks, and the animation cadence a pending one buys.
//!
//! [`request_frame_callback`](zgui_view::ViewHost::request_frame_callback) is how a view drives
//! an animation of its own: register, step, register again. Three properties carry the design,
//! and each is load-bearing.
//!
//! * **A batch is drained before it runs.** A callback that registers again from its own body
//!   lands in the emptied store and waits for the next frame, which is what makes "once per
//!   frame" a property of the seam rather than a discipline every caller has to keep.
//! * **A pending entry makes the window count as animating**, so the frames that run the
//!   callbacks are paced at the display's refresh interval by the same cadence CSS animations
//!   ride — never by a timer a component guessed the display's rate for.
//! * **The moment of the first registration is held**, because the animation cadence only paces
//!   frames that follow frames. A registration made outside any frame — from a resolved future,
//!   from a platform callback — is what this held moment buys the first frame for; every frame
//!   after that is the cadence's.

use std::rc::Rc;
use std::time::Instant;

use zgui_view::FrameRequestId;
use zgui_vocab::Timestamp;

/// One window's pending frame callbacks.
#[derive(Default)]
pub struct FrameCallbacks {
    /// The pending batch, in registration order.
    entries: Vec<(FrameRequestId, Rc<dyn Fn(Timestamp)>)>,
    /// The next identity, never reused.
    next: u64,
    /// When the first entry of the pending batch was registered.
    ///
    /// Held rather than read at the merge, because a frame deadline is a moment something became
    /// owed, never a moment derived from `now` — see the loop's deadline doctrine.
    since: Option<Instant>,
}

impl FrameCallbacks {
    /// A store with nothing pending.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `callback` to run once, during the next frame.
    pub fn request(&mut self, now: Instant, callback: Rc<dyn Fn(Timestamp)>) -> FrameRequestId {
        self.next += 1;
        let id = FrameRequestId::new(self.next);
        self.entries.push((id, callback));
        self.since.get_or_insert(now);
        id
    }

    /// Cancels a pending callback.
    ///
    /// Cancelling one that already ran, or one that was already cancelled, does nothing.
    pub fn cancel(&mut self, id: FrameRequestId) {
        self.entries.retain(|(held, _)| *held != id);
        if self.entries.is_empty() {
            self.since = None;
        }
    }

    /// Whether nothing is pending.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many callbacks are pending.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// When the first entry of the pending batch was registered, for the deadline merge.
    pub fn pending_since(&self) -> Option<Instant> {
        self.since
    }

    /// Takes the pending batch, in registration order.
    ///
    /// The callbacks come back rather than being run here, because running one re-enters the
    /// code that registers them, and a store borrowed across that call is a store borrowed
    /// across arbitrary work.
    pub fn take(&mut self) -> Vec<Rc<dyn Fn(Timestamp)>> {
        self.since = None;
        core::mem::take(&mut self.entries)
            .into_iter()
            .map(|(_, callback)| callback)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Instant;

    use zgui_vocab::Timestamp;

    use super::FrameCallbacks;

    #[test]
    fn a_batch_is_drained_before_it_runs_so_a_re_registration_waits() {
        let mut callbacks = FrameCallbacks::new();
        let ran = Rc::new(Cell::new(0));
        let counter = Rc::clone(&ran);
        callbacks.request(
            Instant::now(),
            Rc::new(move |_| counter.set(counter.get() + 1)),
        );

        let batch = callbacks.take();
        assert!(callbacks.is_empty(), "taken before anything runs");
        for callback in &batch {
            callback(Timestamp::ORIGIN);
        }
        assert_eq!(ran.get(), 1);
        assert!(callbacks.take().is_empty());
    }

    #[test]
    fn cancelling_the_last_entry_clears_the_held_moment() {
        let mut callbacks = FrameCallbacks::new();
        let now = Instant::now();
        let id = callbacks.request(now, Rc::new(|_| {}));
        assert_eq!(callbacks.pending_since(), Some(now));

        callbacks.cancel(id);
        assert!(callbacks.is_empty());
        assert_eq!(
            callbacks.pending_since(),
            None,
            "an empty store owes the loop nothing"
        );
        // Cancelling twice is not an error, and neither is cancelling after the fact.
        callbacks.cancel(id);
    }

    #[test]
    fn the_held_moment_is_the_first_registrations() {
        let mut callbacks = FrameCallbacks::new();
        let first = Instant::now();
        let later = first + core::time::Duration::from_millis(5);
        callbacks.request(first, Rc::new(|_| {}));
        callbacks.request(later, Rc::new(|_| {}));
        assert_eq!(callbacks.pending_since(), Some(first));
        assert_eq!(callbacks.len(), 2);
    }
}
