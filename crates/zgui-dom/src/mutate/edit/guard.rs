//! Opening and closing a batch, including when the body of one panics.
//!
//! A batch is opened by taking the document's write token and raising its depth, and closed by
//! lowering the depth and — at zero — doing the work that was deferred: expanding what the changes
//! to each child list imply about the siblings, handing every element the hint its change earned,
//! and asking for the frame that will show the result.
//!
//! All of that is owned by a guard rather than written at the end of a function, because the middle
//! of a batch is exactly where a panic is plausible: a listener runs application code, and
//! application code panics. A guard runs on the way out either way, so the depth cannot be
//! stranded; and when it is leaving *because* of a panic it does not attempt the deferred work at
//! all, because half a batch's records describe a document that is half changed.

use crate::arena::document::Document;
use crate::mutate::edit::session::Claim;

/// The reason a document will accept no more changes.
///
/// A batch whose body panicked left some of its changes applied and some not, and left the records
/// the style engine compares against describing neither state. Every later change is refused rather
/// than compounding it, which turns a silent, permanently frozen interface into a reported failure.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Poisoned;

impl core::fmt::Display for Poisoned {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("the document was poisoned by a batch of changes that panicked")
    }
}

impl core::error::Error for Poisoned {}

/// Poisons a document if the scope it guards unwinds.
///
/// Used for work that is not a batch of changes and cannot be resumed either — a style traversal
/// whose worker panicked leaves per-element bookkeeping in states no later traversal can interpret,
/// and the worker that panicked holds nothing anyone can inspect. Failing loudly on an internal
/// invariant violation is the decision; carrying on is not a recovery path.
pub(crate) struct PoisonOnUnwind<'doc> {
    /// The document to poison.
    pub(crate) document: &'doc Document,
    /// Where the guarded scope was entered, for the report it leaves behind.
    pub(crate) entered_at: &'static core::panic::Location<'static>,
}

impl Drop for PoisonOnUnwind<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() && self.document.edit_state().poison() {
            tracing::error!(
                scope = %self.entered_at,
                "work over a document panicked; the document is poisoned and will accept no \
                 further changes"
            );
        }
    }
}

/// Holds a batch open and closes it however the body leaves.
pub(crate) struct BatchGuard<'doc> {
    /// The document the batch is open on.
    document: &'doc Document,
    /// Whether this guard is the one that took the write token, and so owes releasing it.
    outermost: bool,
    /// Where the batch was opened, for the report a panicking batch leaves behind.
    opened_at: &'static core::panic::Location<'static>,
}

impl<'doc> BatchGuard<'doc> {
    /// Opens a batch on `document`, joining one that is already open on this thread.
    ///
    /// # Panics
    ///
    /// Panics if another thread holds the document's write token. A document accepts changes from
    /// one thread at a time; two threads changing one document concurrently would race on the
    /// batch's scratch, and failing here is what makes the shared-reference API sound rather than
    /// merely convenient.
    pub(crate) fn open(
        document: &'doc Document,
        opened_at: &'static core::panic::Location<'static>,
    ) -> Result<Self, Poisoned> {
        if document.edit_state().is_poisoned() {
            return Err(Poisoned);
        }
        let outermost = match document.edit_state().claim() {
            Claim::Fresh => true,
            Claim::Nested => false,
            Claim::Contended => panic!(
                "a document accepts changes from one thread at a time, and another thread has a \
                 batch open on this one"
            ),
        };
        // SAFETY: the token is held for the whole of this call and no other reference derived from
        // the cell is live, because nothing between the claim above and the store below can run.
        unsafe { document.edit_state().batch() }.depth += 1;
        Ok(Self {
            document,
            outermost,
            opened_at,
        })
    }
}

impl Drop for BatchGuard<'_> {
    fn drop(&mut self) {
        let state = self.document.edit_state();
        let closing = {
            // SAFETY: the token is still held — it is released at the end of this function — and
            // the borrow is confined to this block, so nothing else derived from the cell is live
            // while it exists.
            let batch = unsafe { state.batch() };
            batch.depth = batch.depth.saturating_sub(1);
            batch.depth == 0
        };

        if std::thread::panicking() {
            // Whatever the depth: a body that unwound left its changes half applied and its records
            // describing neither state, and a caller that catches the unwind inside the batch that
            // encloses it would otherwise close that batch over the wreckage. Only the first call
            // reports it, so a panic through several levels logs once.
            if state.poison() {
                tracing::error!(
                    batch = %self.opened_at,
                    "a batch of document changes panicked; the document is poisoned and will \
                     accept no further changes"
                );
            }
            if self.outermost {
                state.release();
            }
            return;
        }

        if closing {
            self.document.close_batch();
        }
        if self.outermost {
            state.release();
        }
    }
}
