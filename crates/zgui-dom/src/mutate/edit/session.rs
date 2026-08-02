//! The state one document carries between and during batches of changes.
//!
//! Everything here is reached through a *shared* reference to the document, because every method a
//! view calls takes one: a listener that changes the document during dispatch is holding a shared
//! handle, and so is the focus manager writing a state bit while the input layer is already inside
//! a batch. Requiring an exclusive borrow to change a document would turn all three into a runtime
//! borrow failure in ordinary use.
//!
//! Three mechanisms make that safe rather than merely convenient.
//!
//! **A single-writer token.** The first thread to open a batch records itself and every other
//! thread's attempt fails rather than proceeding, so the scratch behind the cell has one writer at a
//! time; the token is released and acquired with the orderings that make one thread's batch happen
//! before the next thread's.
//!
//! **A depth counter, owned by a guard.** Opening a batch inside an open one joins it rather than
//! starting a second, and the work owed at the close of a batch runs once, when the depth returns to
//! zero. The counter belongs to a guard so that a body which unwinds cannot leave it non-zero — a
//! stranded counter means every later batch joins one that never closes, and the end-of-batch work
//! never runs again. The symptom is an interface that runs, accepts input and silently never
//! updates.
//!
//! **A poison flag.** A batch that unwound left the document in a state nothing can interpret: some
//! of a change applied, some records taken, some not. Rather than carry on, the document refuses
//! every later change and says so, so that the failure surfaces where it happened.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::mutate::hints::HintLog;
use crate::mutate::snapshot::SnapshotStore;
use crate::mutate::structure::StructureLog;

/// The scratch one open batch accumulates, plus the records the next restyle consumes.
#[derive(Default)]
pub(crate) struct Batch {
    /// How many nested calls are inside the open batch, or zero when none is open.
    pub(crate) depth: u32,
    /// What each changed element looked like before it changed.
    pub(crate) snapshots: SnapshotStore,
    /// Which parents' child lists changed, and what that can have affected.
    pub(crate) structure: StructureLog,
    /// How much work each changed element's style needs redone.
    pub(crate) hints: HintLog,
    /// The roots of the subtrees that left the document, in the order they left.
    ///
    /// Kept because nothing downstream can recover them: the stage that works out what changed
    /// compares output that still exists, and a removed subtree's output does not, so the area it
    /// vacated is never anyone's previous extent. A removed node's columns — its boxes among them —
    /// survive until the frame's recycling pass, so this list is enough to find them.
    pub(crate) removed: Vec<crate::id::node_key::NodeIndex>,
    /// The same roots again, for the recycling pass rather than for the damage one.
    ///
    /// Two lists rather than one because the two consumers cannot share: the damage stage runs
    /// during the frame and *takes* what it reads, so a recycling pass reading the same list would
    /// find it empty and drop nothing at all — every removed record, key and column row kept for
    /// the life of the document, with the damage still correct and nothing to notice by. They are
    /// filled together, in [`Edit::remove`](crate::Edit::remove), and each is emptied by its own
    /// consumer.
    pub(crate) detached: Vec<crate::id::node_key::NodeIndex>,
}

/// Everything a document needs to accept changes through a shared reference.
pub struct EditState {
    /// The open batch's scratch.
    batch: UnsafeCell<Batch>,
    /// Which thread holds the write token, or zero when nobody does.
    owner: AtomicU64,
    /// Whether a batch has unwound, after which the document accepts no more changes.
    poisoned: AtomicBool,
    /// Whether a change is waiting for a frame that has not been asked for yet.
    redraw: AtomicBool,
    /// Whether a frame is currently being produced.
    in_frame: AtomicBool,
    /// Whether a change arrived during a frame, so another one is owed after it.
    another_frame: AtomicBool,
    /// The next listener identity to hand out. Never rewound, so an identity is never reused and
    /// a handler removed mid-dispatch cannot be confused with one registered afterwards.
    next_listener: AtomicU64,
}

// SAFETY: the only field that is not already `Sync` is the batch scratch, and access to it is
// serialised by the token above: a thread reaches it only while `owner` names that thread, the
// token is stored with `Release` when a batch closes and taken with `Acquire` when one opens, and
// no thread that fails to take it proceeds. So one thread's writes to the scratch happen before the
// next thread's reads of it, and no two threads are ever inside it at once.
unsafe impl Sync for EditState {}

impl EditState {
    /// A document that has never been changed.
    pub(crate) fn new() -> Self {
        Self {
            batch: UnsafeCell::new(Batch::default()),
            owner: AtomicU64::new(NOBODY),
            poisoned: AtomicBool::new(false),
            redraw: AtomicBool::new(false),
            in_frame: AtomicBool::new(false),
            another_frame: AtomicBool::new(false),
            next_listener: AtomicU64::new(1),
        }
    }

    /// Mints a listener identity that this document has never issued before.
    pub(crate) fn next_listener(&self) -> crate::side::listeners::ListenerId {
        crate::side::listeners::ListenerId::new(self.next_listener.fetch_add(1, Ordering::Relaxed))
    }

    /// Whether a batch has unwound and left this document unusable.
    pub(crate) fn is_poisoned(&self) -> bool {
        self.poisoned.load(Ordering::Acquire)
    }

    /// Marks the document unusable. Idempotent, and reports whether this call was the first.
    pub(crate) fn poison(&self) -> bool {
        !self.poisoned.swap(true, Ordering::AcqRel)
    }

    /// Whether a batch is open on this thread.
    pub(crate) fn is_open(&self) -> bool {
        self.owner.load(Ordering::Relaxed) == thread_token()
    }

    /// The batch scratch.
    ///
    /// # Safety
    ///
    /// The caller must hold the write token, and must not hold another reference derived from this
    /// cell — including across any call that can open a nested batch.
    #[allow(
        clippy::mut_from_ref,
        reason = "the caller's obligation is stated above"
    )]
    pub(crate) unsafe fn batch(&self) -> &mut Batch {
        // SAFETY: the caller's obligation, discharged by every caller being a method of the batch
        // guard or of `Edit`, both of which exist only while the token is held.
        unsafe { &mut *self.batch.get() }
    }

    /// Takes the write token for this thread, or reports who already has it.
    ///
    /// Reports [`Claim::Nested`] when this thread already holds it, which is a batch opened inside
    /// an open one and joins rather than failing.
    pub(crate) fn claim(&self) -> Claim {
        let me = thread_token();
        match self
            .owner
            .compare_exchange(NOBODY, me, Ordering::Acquire, Ordering::Acquire)
        {
            Ok(_) => Claim::Fresh,
            Err(held) if held == me => Claim::Nested,
            Err(_) => Claim::Contended,
        }
    }

    /// Gives the write token back.
    pub(crate) fn release(&self) {
        self.owner.store(NOBODY, Ordering::Release);
    }

    /// Records that a change is waiting for a frame.
    ///
    /// During a frame this instead records that another frame is owed, because the stage that ends
    /// a frame is the one place a redraw request can be issued without producing a second, empty
    /// frame for every stage that asked.
    pub(crate) fn request_redraw(&self) {
        if self.in_frame.load(Ordering::Relaxed) {
            self.another_frame.store(true, Ordering::Relaxed);
        } else {
            self.redraw.store(true, Ordering::Relaxed);
        }
    }

    /// Whether a frame has been asked for and not yet taken.
    pub(crate) fn redraw_requested(&self) -> bool {
        self.redraw.load(Ordering::Relaxed)
    }

    /// Takes the pending redraw request, reporting whether there was one.
    pub(crate) fn take_redraw_request(&self) -> bool {
        self.redraw.swap(false, Ordering::Relaxed)
    }

    /// Marks the start of a frame, so that changes made inside it defer their request.
    pub(crate) fn begin_frame(&self) {
        self.in_frame.store(true, Ordering::Relaxed);
        self.another_frame.store(false, Ordering::Relaxed);
    }

    /// Records that everything changed so far is about to be serviced by the frame in flight.
    pub(crate) fn changes_serviced(&self) {
        self.another_frame.store(false, Ordering::Relaxed);
    }

    /// Marks the end of a frame and reports whether anything changed during it.
    pub(crate) fn end_frame(&self) -> bool {
        self.in_frame.store(false, Ordering::Relaxed);
        self.another_frame.swap(false, Ordering::Relaxed)
    }
}

impl Default for EditState {
    fn default() -> Self {
        Self::new()
    }
}

/// What happened when a thread asked for the write token.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Claim {
    /// The token was free and is now held by this thread.
    Fresh,
    /// This thread already held it, so this batch joins the open one.
    Nested,
    /// Another thread holds it.
    Contended,
}

/// The token value meaning "nobody holds it".
const NOBODY: u64 = 0;

/// This thread's token, a small non-zero number allocated the first time it is asked for.
fn thread_token() -> u64 {
    /// The next token to hand out. One rather than zero, because zero means nobody.
    static NEXT: AtomicU64 = AtomicU64::new(1);
    thread_local! {
        static TOKEN: u64 = NEXT.fetch_add(1, Ordering::Relaxed);
    }
    TOKEN.with(|token| *token)
}

#[cfg(test)]
mod tests {
    use super::{Claim, EditState, thread_token};

    #[test]
    fn a_thread_keeps_one_token_and_no_two_threads_share_one() {
        let mine = thread_token();
        assert_eq!(mine, thread_token());
        let theirs = std::thread::spawn(thread_token).join().expect("it ran");
        assert_ne!(mine, theirs);
    }

    #[test]
    fn claiming_twice_on_one_thread_is_a_nested_batch() {
        let state = EditState::new();
        assert_eq!(state.claim(), Claim::Fresh);
        assert_eq!(state.claim(), Claim::Nested);
        state.release();
        assert_eq!(state.claim(), Claim::Fresh);
    }

    #[test]
    fn a_second_thread_is_told_the_token_is_taken_rather_than_racing_for_it() {
        let state = EditState::new();
        assert_eq!(state.claim(), Claim::Fresh);
        std::thread::scope(|scope| {
            let held = &state;
            let outcome = scope.spawn(move || held.claim()).join().expect("it ran");
            assert_eq!(outcome, Claim::Contended);
        });
    }

    #[test]
    fn a_redraw_asked_for_during_a_frame_becomes_a_frame_owed_after_it() {
        let state = EditState::new();
        state.begin_frame();
        state.request_redraw();
        assert!(
            !state.redraw_requested(),
            "asking inside a frame would produce one redundant empty frame per requester"
        );
        assert!(state.end_frame());

        state.request_redraw();
        assert!(state.redraw_requested());
        assert!(state.take_redraw_request());
        assert!(!state.take_redraw_request());
    }

    #[test]
    fn poisoning_is_idempotent_and_only_the_first_call_reports_it() {
        let state = EditState::new();
        assert!(!state.is_poisoned());
        assert!(state.poison());
        assert!(!state.poison());
        assert!(state.is_poisoned());
    }
}
