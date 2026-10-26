//! The once-per-frame flush, and the state that makes it re-entrant-safe.

use std::cell::RefCell;

use crate::executor::assert::assert_ui_thread;
use crate::executor::{context, pool};

/// What a flush did, and what it owes the frame that follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlushOutcome {
    /// Whether a redraw must be requested for the work this flush created.
    ///
    /// An effect that writes a signal makes another effect ready, but the wake it raises is
    /// suppressed — asking the platform for a redraw from inside the frame that is already
    /// running would be a wake per write — and serviced by the flush itself where it can be. The
    /// frame loop requests exactly one redraw when this is true, which is what carries a wake the
    /// flush stopped polling before, and a task set aside by the iteration budget, into the next
    /// frame.
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
    ///
    /// A wake raised by a task the pool is polling is serviced by the same poll, so a wake alone
    /// owes nothing. Only a wake the flush stopped polling before — the pool stalled and something
    /// woke a task afterwards — becomes a frame.
    woke: bool,
    /// Whether any task was set aside by the budget in this flush.
    budget_exhausted: bool,
}

thread_local! {
    static FLUSH: RefCell<FlushState> = RefCell::new(FlushState::default());
}

/// Runs every ready reactive task to a stall and every marked [`Sink`](crate::Sink), and reports
/// what the frame still owes.
///
/// This is the one place reactive work happens. Writing a signal marks its observers and wakes
/// their tasks; nothing runs until the frame loop calls this. Each round polls the task pool to a
/// stall, then runs the marked sinks in the order they were marked. An effect therefore sees what
/// a sink wrote in this flush only from the next flush, and each step from a sink to a task uses
/// one of the flush's rounds. Because the notification
/// behind each task or sink holds one slot, a signal written a thousand times between two flushes
/// costs one re-run, not a thousand.
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
        state.woke = false;
        state.budget_exhausted = false;
        true
    });
    if !started {
        return FlushOutcome::default();
    }

    let mut woke_after_stall = false;
    {
        // Clears the flag even if a task panics. Left set, it would make every later flush look
        // re-entrant and silently do nothing, which is a frozen window with no further error.
        let _running = Running;
        // Before the pool, so what a closure posted from another thread writes is settled by this
        // frame rather than by the next one — the same ordering the timer heap already uses.
        crate::task::drain_ui_queue();
        // The pool polls every task woken while it runs, so an effect writing a signal another
        // effect reads is settled in this same poll. What a poll cannot see is a wake that lands
        // between its stall and its return; polling again is the one thing that tells the two
        // apart, and a poll with nothing to do costs a queue check. The rounds are bounded so
        // that a task waking itself from outside the budget cannot hold the frame.
        for round in 0..MAX_ROUNDS {
            // Sinks run after the pool stalls, so an effect that replaces a subtree drops the
            // sinks it took away before they run.
            context::enter(|| {
                pool::poll();
                crate::sink::drain();
            });
            let woke = FLUSH.with_borrow_mut(|state| std::mem::take(&mut state.woke));
            if !woke {
                break;
            }
            if round + 1 == MAX_ROUNDS {
                woke_after_stall = true;
            }
        }
    }

    // A task or sink the budget set aside goes back in its queue without asking for a redraw;
    // `needs_another_frame` is what gets it run.
    let tasks_deferred = pool::release_deferred();
    let sinks_deferred = crate::sink::release_deferred();
    FLUSH.with_borrow(|state| FlushOutcome {
        needs_another_frame: woke_after_stall || tasks_deferred || sinks_deferred,
        budget_exhausted: state.budget_exhausted,
    })
}

/// How many times one flush polls the pool before a wake it keeps seeing is left to the next
/// frame.
const MAX_ROUNDS: u32 = 3;

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
                state.woke = true;
            }
            state.running
        })
        .unwrap_or(true)
}

/// Whether a flush is running on this thread.
///
/// A sink marked during the flush runs in that flush, so the mark asks for no frame. A thread that
/// is tearing down answers `true` for the reason [`note_wake`] gives.
pub(crate) fn is_running() -> bool {
    FLUSH
        .try_with(|state| state.borrow().running)
        .unwrap_or(true)
}

/// Records that the budget set a task or a sink aside in this flush.
pub(crate) fn note_exhausted() {
    FLUSH.with_borrow_mut(|state| state.budget_exhausted = true);
}
