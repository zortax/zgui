//! Changing a document that something has already looked at.
//!
//! Every change goes through one API, and the API is a *batch* rather than a set of setters,
//! because a change is only half of what has to happen. The other half is bookkeeping the style
//! engine cannot do for an embedder: recording what the element was before, telling the ancestors
//! there is work below them, deciding how much of the engine's work the change actually needs
//! redone, and — for a change to a child list — working out which *other* children can no longer
//! match what they matched. Every one of those is easy to leave out and silently does nothing when
//! it is left out, so none of them is optional and none of them is exposed separately.
//!
//! ```
//! use zgui_dom::{Document, EverythingMatters, NodeKind};
//! use zgui_interned::{ClassName, ElementName};
//!
//! let mut document = Document::new();
//! let root = document.append(
//!     document.document_index(),
//!     NodeKind::Element,
//!     ElementName::new("root"),
//! );
//!
//! document
//!     .edit(&EverythingMatters, |edit| {
//!         let row = edit.create_element(ElementName::new("li"));
//!         edit.add_class(row, ClassName::new("row"));
//!         edit.insert_before(root, row, None);
//!     })
//!     .expect("the document has not been poisoned");
//!
//! assert_eq!(document.store().core(root).child_count(), 1);
//! assert!(document.take_redraw_request(), "a change asks for the frame that will show it");
//! ```
//!
//! | Module | Contents |
//! |---|---|
//! | [`session`] | the state a document carries so it can be changed through a shared reference |
//! | [`guard`] | opening and closing a batch, including when its body panics |
//! | [`build`] | creating nodes, and linking them in and out |
//! | [`attrs`] | identifiers, classes and attributes |
//! | [`interaction`] | interaction state, and what a view is watching |
//! | [`content`] | text, semantics and imperative properties |
//! | [`style`] | the declarations an element carries of its own |
//! | [`custom_state`] | the states an author named, and the invalidation they bring |
//! | [`listeners`] | which events a node listens for |

pub mod attrs;
pub mod build;
pub mod content;
pub mod custom_state;
pub mod guard;
pub mod interaction;
pub mod listeners;
pub mod session;
pub mod style;

use crate::arena::document::Document;
use crate::arena::store::DocumentStore;
use crate::mutate::edit::guard::{BatchGuard, PoisonOnUnwind, Poisoned};
use crate::mutate::edit::session::{Batch, EditState};
use crate::mutate::filter::StyleFilter;

/// An open batch of changes to one document.
///
/// Handed to the body of [`Document::edit`] and never held past it. Each method applies one change
/// and everything that change owes; work that only makes sense once — expanding what a change to a
/// child list implies about the other children, handing out the accumulated hints, asking for a
/// frame — happens when the outermost batch closes.
pub struct Edit<'doc> {
    /// The document being changed.
    document: &'doc Document,
    /// Which changes can affect a computed style, answered from the active rule set.
    filter: &'doc dyn StyleFilter,
}

impl<'doc> Edit<'doc> {
    /// The document this batch is changing.
    pub fn document(&self) -> &'doc Document {
        self.document
    }

    /// The store, exclusively, for the duration of one call.
    ///
    /// Re-derived per call and never held across anything that could open a nested batch, which is
    /// what keeps a nested change from invalidating an outer one's borrow.
    fn store(&mut self) -> &mut DocumentStore {
        let document = self.document;
        // SAFETY: the batch guard holds the document's write token, so no other thread is inside a
        // batch; no reference into the store outlives the call that derives it; and the style
        // traversal, which is the only other reader, may not run while a batch is open.
        unsafe { document.store_exclusive() }
    }

    /// The store and the batch scratch together, for the steps that need both.
    fn parts(&mut self) -> (&mut DocumentStore, &mut Batch) {
        let document = self.document;
        // SAFETY: as `store` and `batch`, and the two references are into separate allocations —
        // the store is owned through a pointer of its own and the scratch lives on the document.
        unsafe { (document.store_exclusive(), document.edit_state().batch()) }
    }

    /// Which interaction-state bits any active selector could match `node` on.
    fn watched_states(&mut self, node: crate::id::node_key::NodeIndex) -> stylo_dom::ElementState {
        let filter = self.filter;
        self.store().states_for(node, filter)
    }
}

impl Document {
    /// Applies a batch of changes, maintaining everything the style engine needs to invalidate.
    ///
    /// Takes a shared reference, so a listener may change the document from inside a dispatch that
    /// is itself running inside a batch: a nested call joins the batch already open rather than
    /// starting a second one, and the work owed at the close of a batch runs once, when the
    /// outermost call returns.
    ///
    /// `filter` answers whether a given change can affect any computed style at all. A change it
    /// rejects is applied without recording anything and without entering the style engine. Pass
    /// [`EverythingMatters`](crate::EverythingMatters) when there is no rule set to answer from.
    ///
    /// # Errors
    ///
    /// Returns [`Poisoned`] if an earlier batch on this document panicked. Such a document is left
    /// half-changed, with records describing neither the old state nor the new, and no later change
    /// can repair it.
    ///
    /// # Panics
    ///
    /// Panics if another thread has a batch open on this document.
    #[track_caller]
    pub fn edit<R>(
        &self,
        filter: &dyn StyleFilter,
        body: impl FnOnce(&mut Edit<'_>) -> R,
    ) -> Result<R, Poisoned> {
        let guard = BatchGuard::open(self, core::panic::Location::caller())?;
        let mut edit = Edit {
            document: self,
            filter,
        };
        let outcome = body(&mut edit);
        drop(guard);
        Ok(outcome)
    }

    /// Runs `body`, poisoning this document if it unwinds.
    ///
    /// For work that is not a batch of changes and cannot be resumed either: a style traversal
    /// whose worker panics leaves per-element bookkeeping in a state no later traversal can
    /// interpret, so the document is poisoned and the panic is allowed to continue outwards rather
    /// than being turned into a document that looks usable and is not.
    ///
    /// # Errors
    ///
    /// Returns [`Poisoned`] without running `body` if the document is poisoned already.
    #[track_caller]
    pub fn guarded<R>(&self, body: impl FnOnce() -> R) -> Result<R, Poisoned> {
        if self.is_poisoned() {
            return Err(Poisoned);
        }
        let guard = PoisonOnUnwind {
            document: self,
            entered_at: core::panic::Location::caller(),
        };
        let outcome = body();
        drop(guard);
        Ok(outcome)
    }

    /// Whether a batch of changes is open on this thread.
    ///
    /// A change made while this is true joins that batch: its end-of-batch work runs once, when the
    /// outermost call returns.
    pub fn is_editing(&self) -> bool {
        self.edit_state().is_open()
    }

    /// Whether an earlier batch of changes panicked and left this document unusable.
    pub fn is_poisoned(&self) -> bool {
        self.edit_state().is_poisoned()
    }

    /// Whether a change is waiting for a frame that nothing has produced yet.
    pub fn redraw_requested(&self) -> bool {
        self.edit_state().redraw_requested()
    }

    /// Takes the pending redraw request, reporting whether there was one.
    ///
    /// Called by whatever drives frames, which asks the platform for one only when this says a
    /// change is waiting.
    pub fn take_redraw_request(&self) -> bool {
        self.edit_state().take_redraw_request()
    }

    /// Records that a frame is being produced.
    ///
    /// While a frame is in flight a change records that *another* frame is owed rather than asking
    /// for one, so a frame whose own stages change the document costs one extra frame in total
    /// rather than one per stage that changed something.
    pub fn begin_frame(&self) {
        self.edit_state().begin_frame();
    }

    /// Records that the frame in flight is about to service everything changed so far.
    ///
    /// A frame drains its events, fires its timers and flushes its reactive graph before it styles
    /// anything, so every document change made in those stages is shown by the very frame that made
    /// it. Without this the flag they set was still standing at the end of that frame, and every
    /// interaction cost a second frame that damaged nothing and presented an identical surface.
    ///
    /// Called once, by the frame, at the moment the stages that consume changes begin. A change
    /// made after it — an observation handler writing a signal, a scroll dispatched from the laid
    /// out geometry — sets the flag again and is honoured, which is the whole reason this is a
    /// point in the frame rather than a rule about who is asking.
    pub fn changes_serviced(&self) {
        self.edit_state().changes_serviced();
    }

    /// Records that the frame has ended, and reports whether anything changed during it.
    pub fn end_frame(&self) -> bool {
        self.edit_state().end_frame()
    }

    /// Retires every obligation in `phase`, as the stage that serviced it would have.
    ///
    /// Each stage of a frame retires what it consumed as a side effect of doing its work, and the
    /// record of *which children owed it* is rewritten on the way back up. A stage that is not
    /// running leaves both behind, and the next walk over the document then prices in every
    /// element the last one touched — the cost is invisible in what the walk *serviced* and shows
    /// up only in what it probed.
    ///
    /// So this exists for the two callers that have to retire without servicing: a frame whose
    /// later stages are switched off, and a test that runs some of them. It is not a way to
    /// discard work — anything retired here simply does not happen.
    ///
    /// ```
    /// use zgui_bits::Dirty;
    /// use zgui_dom::{Document, EverythingMatters, NodeKind};
    /// use zgui_interned::ElementName;
    /// use zgui_vocab::UiState;
    ///
    /// let mut document = Document::new();
    /// let root = document
    ///     .edit(&EverythingMatters, |edit| {
    ///         let node = edit.create_element(ElementName::new("root"));
    ///         edit.insert_before(document.document_index(), node, None);
    ///         node
    ///     })
    ///     .expect("not poisoned");
    /// document
    ///     .edit(&EverythingMatters, |edit| {
    ///         edit.set_state(root, UiState::HOVER, true);
    ///     })
    ///     .expect("not poisoned");
    /// assert!(!document.store().core(root).dirty().own().is_clean());
    ///
    /// document.retire(Dirty::all());
    /// assert!(document.store().core(root).dirty().own().is_clean());
    /// ```
    pub fn retire(&mut self, phase: zgui_bits::Dirty) {
        let root = self.document_index();
        crate::dirty::walk::walk(self.store_mut(), root, phase, &mut |_store, _node| {});
    }

    /// Takes the pre-change records the next restyle is to consume, leaving the document empty.
    ///
    /// Taken rather than borrowed so that the restyle owns them for its duration: a change made
    /// while a restyle is running belongs to the *next* restyle, and starts a fresh set here rather
    /// than being added to the set already in flight.
    pub fn take_snapshots(&mut self) -> crate::mutate::snapshot::SnapshotStore {
        // SAFETY: an exclusive borrow of the document rules out any other reference into the cell.
        core::mem::take(&mut unsafe { self.edit_state().batch() }.snapshots)
    }

    /// Takes the roots of the subtrees removed since this was last called.
    ///
    /// The area a removed subtree occupied has to be repainted, and nothing downstream can work out
    /// where it was: what compares output between frames only sees output that still exists. These
    /// are the nodes to read it from, and their columns survive until the frame's recycling pass —
    /// so read them, and everything below them, before calling
    /// [`end_frame`](crate::arena::end_frame), which is where the records go.
    ///
    /// A root that was taken out and put back during the same frame is named here too. It has not
    /// moved anywhere its own damage does not already cover, so repainting where it was costs a
    /// rectangle that is about to be painted anyway; leaving it out would need this to know which
    /// removals the rest of the frame went on to undo.
    pub fn take_removed(&mut self) -> Vec<crate::id::node_key::NodeIndex> {
        // SAFETY: an exclusive borrow of the document rules out any other reference into the cell.
        core::mem::take(&mut unsafe { self.edit_state().batch() }.removed)
    }

    /// How many elements have a pre-change record waiting.
    pub fn pending_snapshots(&self) -> usize {
        // SAFETY: a shared borrow, and the returned value is copied out before anything else can
        // derive a reference from the cell.
        unsafe { self.edit_state().batch() }.snapshots.len()
    }

    /// The state this document carries so that it can be changed through a shared reference.
    pub(crate) fn edit_state(&self) -> &EditState {
        &self.edit
    }

    /// Does the work one batch deferred to its close.
    ///
    /// The order is fixed: what a change to a child list implies about the other children is worked
    /// out first, because that is what decides which siblings need a hint; then every element is
    /// handed the hint it earned; then the frame that will show the result is asked for.
    pub(crate) fn close_batch(&self) {
        {
            // SAFETY: the write token is held by the guard calling this, and the two references are
            // into separate allocations. Both end with this block.
            let (store, batch) = unsafe { (self.store_exclusive(), self.edit_state().batch()) };
            batch.structure.close(store, &mut batch.hints);
            batch.hints.apply(store);
        }
        self.edit_state().request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;

    use crate::arena::document::Document;
    use crate::mutate::filter::EverythingMatters;
    use crate::node::kind::NodeKind;

    #[test]
    fn a_nested_batch_joins_the_open_one_and_the_close_happens_once() {
        let mut document = Document::new();
        document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        document.take_redraw_request();

        document
            .edit(&EverythingMatters, |outer| {
                let document = outer.document();
                document
                    .edit(&EverythingMatters, |_| {
                        assert!(
                            !document.redraw_requested(),
                            "a nested batch closing would have asked for a frame of its own"
                        );
                    })
                    .expect("not poisoned");
                assert!(!document.redraw_requested());
            })
            .expect("not poisoned");
        assert!(document.take_redraw_request());
        assert!(!document.take_redraw_request());
    }

    #[test]
    fn a_batch_that_panics_poisons_the_document_and_the_next_one_says_so() {
        let document = Document::new();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            document.edit(&EverythingMatters, |_| panic!("a listener panicked"))
        }));
        assert!(outcome.is_err());

        assert!(document.is_poisoned());
        assert_eq!(
            document.edit(&EverythingMatters, |_| ()),
            Err(crate::mutate::edit::guard::Poisoned),
            "a document that silently accepted changes after this would never update again"
        );
    }

    #[test]
    fn the_batch_depth_is_recoverable_after_a_panic() {
        // The failure this guards against is not the panic; it is the counter left non-zero, after
        // which every later batch joins one that never closes and no end-of-batch work ever runs.
        let document = Document::new();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            document.edit(&EverythingMatters, |_| panic!("a listener panicked"))
        }));
        assert!(!document.is_editing(), "the write token was given back");
    }
}
