//! The once-per-frame flush, and the state that makes it re-entrant-safe.

use std::cell::RefCell;
use std::task::Waker;

use crate::executor::assert::assert_ui_thread;
use crate::executor::pool;

/// What a flush did, and what it owes the frame that follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlushOutcome {
    /// Whether a redraw must be requested for the work this flush created.
    ///
    /// An effect that writes a signal makes another effect ready, but the wake it raises is
    /// suppressed — asking the platform for a redraw from inside the frame that is already
    /// running would be a wake per write. The frame loop requests exactly one redraw when this
    /// is true, which is also what carries a task set aside by the iteration budget into the
    /// next frame.
    pub needs_another_frame: bool,
    /// Whether at least one task exceeded the per-flush iteration budget.
    ///
    /// True means a dependency cycle was cut so the frame could present. The offending task is
    /// named once, at `error` level, through `tracing`.
    pub budget_exhausted: bool,
}

/// State for one thread's flush.
#[derive(Debug, Default)]
struct FlushState {
    /// Whether a flush is running on this thread right now.
    running: bool,
    /// Increments once per flush; the iteration budget is charged per generation.
    generation: u64,
    /// Set by a wake raised while the flush is running.
    needs_another_frame: bool,
    /// Whether any task was set aside by the budget in this flush.
    budget_exhausted: bool,
    /// Wakers of the tasks the budget set aside, woken once the flush has returned.
    deferred: Vec<Waker>,
}

thread_local! {
    static FLUSH: RefCell<FlushState> = RefCell::new(FlushState::default());
}

/// Runs every ready reactive task to a stall, and reports what the frame still owes.
///
/// This is the one place reactive work happens. Writing a signal marks its observers and wakes
/// their tasks; nothing runs until the frame loop calls this. Because the notification behind
/// each task holds one slot, a signal written a thousand times between two flushes costs one
/// re-run, not a thousand.
///
/// Bounded by the per-flush iteration budget, so two effects writing each other's sources cost
/// a logged error and a presented frame rather than a hang. Re-entrant calls — a flush from
/// inside an effect — do nothing and report nothing owed, rather than corrupting the pool.
///
/// A task that panics propagates the panic to the caller and leaves the executor usable: the
/// next flush polls what is left. Nothing here turns one bad task into a window that never
/// updates again.
///
/// # Panics
///
/// In debug builds, if called off the thread that installed the runtime. In any build, if a task
/// polled by this flush panics.
pub fn flush() -> FlushOutcome {
    assert_ui_thread("flush");

    let started = FLUSH.with_borrow_mut(|state| {
        if state.running {
            return false;
        }
        state.running = true;
        state.generation += 1;
        state.needs_another_frame = false;
        state.budget_exhausted = false;
        true
    });
    if !started {
        return FlushOutcome::default();
    }

    {
        // Clears the flag even if a task panics. Left set, it would make every later flush look
        // re-entrant and silently do nothing, which is a frozen window with no further error.
        let _running = Running;
        pool::poll();
    }

    FLUSH.with_borrow_mut(|state| {
        // Waking a task the budget set aside re-queues it in the pool without asking for a
        // redraw; `needs_another_frame` is what gets it polled.
        let deferred = std::mem::take(&mut state.deferred);
        if !deferred.is_empty() {
            state.needs_another_frame = true;
        }
        for waker in deferred {
            waker.wake();
        }
        FlushOutcome {
            needs_another_frame: state.needs_another_frame,
            budget_exhausted: state.budget_exhausted,
        }
    })
}

/// Marks a flush as running for as long as it is alive, however it ends.
struct Running;

impl Drop for Running {
    fn drop(&mut self) {
        let _ = FLUSH.try_with(|state| state.borrow_mut().running = false);
    }
}

/// The flush a poll belongs to. Zero before the first flush.
pub(crate) fn generation() -> u64 {
    FLUSH
        .try_with(|state| state.borrow().generation)
        .unwrap_or_default()
}

/// Records a wake, returning whether it was suppressed rather than forwarded.
///
/// A wake raised on a thread that is tearing down — the last tasks in a pool being dropped as
/// the thread exits, which drops the notification channels they hold — is suppressed too: there
/// is no frame left to ask for, and the state that would answer the question is already gone.
pub(crate) fn note_wake() -> bool {
    FLUSH
        .try_with(|state| {
            let mut state = state.borrow_mut();
            if state.running {
                state.needs_another_frame = true;
            }
            state.running
        })
        .unwrap_or(true)
}

/// Sets a budget-exhausted task aside until the flush has returned.
pub(crate) fn defer(waker: Waker) {
    FLUSH.with_borrow_mut(|state| {
        state.budget_exhausted = true;
        state.deferred.push(waker);
    });
}
