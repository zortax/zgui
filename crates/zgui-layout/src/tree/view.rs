//! The store as one layout pass holds it.
//!
//! A serial pass owns the store exclusively, which is what every accessor has always assumed. A
//! parallel batch cannot: before the workers start, the executor splits the store into its
//! structural half and its layout column, carves one exclusive borrow per subtree box out of the
//! column, and hands each worker the structure plus its own boxes' borrows. Writes land in the
//! store directly — there is no copy on the way in and no commit on the way out, which is what
//! makes a worker's box cost what a serial pass's box costs.
//!
//! # What a worker may touch
//!
//! - Its own subtrees' per-box layout state, through the borrows it was handed. Reaching for a
//!   box it was not handed is a partitioning bug, and panics.
//! - Paragraph identifiers. A resolution pairs every identifier with the shaping key it names,
//!   so a worker records a marker and the executor re-interns each one from its key afterwards,
//!   in request order — the numbering a serial pass would have produced.
//! - Counters, which are process-wide relaxed atomics already and need nothing here.
//!
//! Everything else — box structure, styles, rosters, fragments — is read-only during a batch
//! through [`Structure`], and a worker reaching for the whole store is a bug this module turns
//! into a panic rather than a silent corruption.

use rustc_hash::FxHashMap;
use zgui_dom::side::BoxKey;

use crate::fragment::ParagraphId;
use crate::inline::content::memo::Flattened;
use crate::inline::resolved::InlineResolution;
use crate::tree::store::state::BoxLayout;
use crate::tree::store::{LayoutStore, Structure};

/// The identifier a worker records where a paragraph identifier belongs.
///
/// Never read back: every place a resolution carries an identifier it carries the shaping key
/// beside it, and the executor replaces every marker from the key. The value exists so that a
/// marker escaping into a frame is unmistakable in a transcript.
pub(crate) const PROVISIONAL_PARAGRAPH: ParagraphId = ParagraphId(u32::MAX);

/// How a pass reaches the store.
#[derive(Debug)]
pub(crate) enum StoreView<'a> {
    /// The pass owns the store, exclusively, for its whole duration.
    Exclusive(&'a mut LayoutStore),
    /// The pass is one worker of a batch, over the split store.
    Worker(WorkerStore<'a>),
}

/// One worker's view: the shared structure, and its own boxes' exclusive borrows.
#[derive(Debug)]
pub(crate) struct WorkerStore<'a> {
    /// The structural half every worker reads.
    structure: Structure<'a>,
    /// The layout state of this worker's subtrees, one exclusive borrow per box.
    states: FxHashMap<u32, &'a mut BoxLayout>,
}

impl<'a> WorkerStore<'a> {
    /// A view over the split store, owning the given boxes' state for the batch.
    pub(crate) fn new(structure: Structure<'a>, states: FxHashMap<u32, &'a mut BoxLayout>) -> Self {
        Self { structure, states }
    }
}

impl<'a> StoreView<'a> {
    /// The store, whole, which only an exclusive pass holds.
    ///
    /// # Panics
    ///
    /// In worker mode. Structure reads go through [`StoreView::structure`]; per-box state goes
    /// through the accessors below; anything needing more is a path the batch design missed.
    pub(crate) fn get(&self) -> &LayoutStore {
        match self {
            Self::Exclusive(store) => store,
            Self::Worker(_) => {
                panic!("a layout worker asked for the whole store; see tree::view")
            }
        }
    }

    /// The store, for writing, which only an exclusive pass may do.
    ///
    /// # Panics
    ///
    /// In worker mode, for the same reason as [`StoreView::get`].
    pub(crate) fn get_mut(&mut self) -> &mut LayoutStore {
        match self {
            Self::Exclusive(store) => store,
            Self::Worker(_) => {
                panic!("a layout worker took the whole store mutably; see tree::view")
            }
        }
    }

    /// The structural half of the store, readable in both modes.
    pub(crate) fn structure(&self) -> Structure<'_> {
        match self {
            Self::Exclusive(store) => store.structure(),
            Self::Worker(worker) => worker.structure,
        }
    }

    /// One box's layout state.
    ///
    /// # Panics
    ///
    /// In worker mode, for a box the worker was not handed: state access outside the assigned
    /// subtrees is a partitioning bug and must not read as "no state".
    pub(crate) fn state(&self, key: BoxKey) -> Option<&BoxLayout> {
        match self {
            Self::Exclusive(store) => store.state(key),
            Self::Worker(worker) => Some(
                *worker
                    .states
                    .get(&key.index())
                    .expect("a worker reads only the boxes it was handed"),
            ),
        }
    }

    /// One box's layout state for writing.
    ///
    /// # Panics
    ///
    /// In worker mode, for a box the worker was not handed.
    pub(crate) fn state_mut(&mut self, key: BoxKey) -> &mut BoxLayout {
        match self {
            Self::Exclusive(store) => store.state_mut(key),
            Self::Worker(worker) => worker
                .states
                .get_mut(&key.index())
                .expect("a worker writes only the boxes it was handed"),
        }
    }

    /// What one box measured on one axis, as the intrinsic pre-pass recorded it.
    pub(crate) fn intrinsic(
        &self,
        key: BoxKey,
        axis: crate::axis::Axis,
    ) -> Option<crate::style::convert::length::IntrinsicSizes> {
        self.state(key)?.intrinsic[axis.index()]
    }

    /// Which axes of one box reserve a scrollbar gutter by layout's own decision.
    pub(crate) fn reserved_gutter(&self, key: BoxKey) -> (bool, bool) {
        let Some(state) = self.state(key) else {
            return (false, false);
        };
        let held = state.auto_scroll;
        match state.scroll_lock {
            Some(locked) => (held.0 || locked.0, held.1 || locked.1),
            None => held,
        }
    }

    /// The flattened form one box is holding.
    pub(crate) fn flattened(&self, key: BoxKey) -> Option<&Flattened> {
        self.state(key)?.flattened.as_deref()
    }

    /// Holds a box's flattened form.
    pub(crate) fn hold_flattened(&mut self, key: BoxKey, flattened: Flattened) {
        match self {
            Self::Exclusive(store) => store.hold_flattened(key, flattened),
            Self::Worker(_) => {
                self.state_mut(key).flattened = Some(Box::new(flattened));
            }
        }
    }

    /// The lines one box resolved to.
    pub(crate) fn inline_resolution(&self, key: BoxKey) -> Option<&InlineResolution> {
        self.state(key)?.inline.as_deref()
    }

    /// Records what one inline formatting context resolved to.
    ///
    /// The exclusive form does the paragraph retain-and-release bookkeeping; a worker writes in
    /// place and the executor reconstructs the accounting afterwards from the snapshots it took
    /// before the batch.
    pub(crate) fn set_inline_resolution(&mut self, key: BoxKey, resolution: InlineResolution) {
        match self {
            Self::Exclusive(store) => store.set_inline_resolution(key, resolution),
            Self::Worker(_) => {
                self.state_mut(key).inline = Some(Box::new(resolution));
            }
        }
    }

    /// The identifier a shaped paragraph is carried by.
    ///
    /// A worker records the marker; the executor re-interns from the key beside it.
    pub(crate) fn intern_paragraph(&mut self, key: zgui_text::ParagraphKey) -> ParagraphId {
        match self {
            Self::Exclusive(store) => store.intern_paragraph(key),
            Self::Worker(_) => PROVISIONAL_PARAGRAPH,
        }
    }
}
