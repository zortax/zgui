//! The boxes whose *style* puts them in a class a pass has to visit.
//!
//! Two passes over the document ask the same shape of question. The intrinsic pre-pass wants the
//! boxes written with a content keyword; the `overflow: auto` fixpoint wants the boxes whose gutter
//! is undecided. Both used to answer it by walking every box in the document and testing its style,
//! twice a frame — a cost that scales with how large the document is rather than with how many
//! boxes are in the class, which in both cases is normally none at all.
//!
//! So the answer is maintained instead of recomputed. Each class is a `Vec<BoxKey>` giving the
//! iteration order, plus a membership bit stored on the box's own
//! [`BoxLayout`](crate::tree::store::state::BoxLayout).
//!
//! # The bit is authoritative and the list is a hint
//!
//! The list may hold stale entries: a box that has since been removed, or one whose style was
//! rewritten to something outside the class. It may never *miss* a member — that is the one
//! direction that is a bug, and it is what [`LayoutStore::classify`] exists to prevent by being the
//! only place a box's style is established or changed.
//!
//! Consumers therefore treat the list as a superset and re-test the bit as they go, compacting the
//! entries that no longer belong. A stale entry costs one lookup and then disappears; a missing one
//! would cost a keyword that is never measured, and no assertion would see it. Growth is bounded
//! because a box is pushed only as its bit goes from false to true, and every use compacts.
//!
//! # Reachability
//!
//! The walk this replaces started at the root, so it saw only boxes still attached to the tree. The
//! roster holds every box that was ever *inserted*, attached or not. That difference matters to the
//! gutter fixpoint, which marks the boxes it revises dirty: doing so on a box whose parent chain
//! does not reach the root marks nothing the root can see, and buys a second layout pass that
//! changes nothing. Compaction drops entries whose key names no live box, which covers the case
//! that actually arises — a detached subtree is removed, not merely unlinked.

use zgui_dom::side::BoxKey;

/// The boxes of each style-defined class, in the order a pass visits them.
#[derive(Debug, Default)]
pub(crate) struct Rosters {
    /// Boxes with a content keyword on at least one axis.
    pub(crate) content: Vec<BoxKey>,
    /// Boxes whose overflow is undecided on at least one axis.
    pub(crate) overflow: Vec<BoxKey>,
}

/// A roster taken out of the store for the duration of one pass over it.
///
/// Taken rather than borrowed because every consumer of a roster modifies the store while walking
/// it — the gutter fixpoint writes a decision and marks boxes dirty, the pre-pass writes
/// measurements — and a borrow of one field of the store cannot be held across that.
///
/// The store keeps registering into the emptied list while this is out. [`Roster::restore`]
/// therefore puts the survivors back *alongside* whatever accumulated rather than over it, and no
/// entry can be duplicated by doing so: a box already in the taken list still has its membership
/// bit set, and registration pushes only on the transition into the class.
#[derive(Debug)]
pub(crate) struct Roster {
    /// The entries, which the holder compacts in place.
    pub(crate) entries: Vec<BoxKey>,
}

impl Roster {
    /// Takes ownership of a list, leaving the store's own empty.
    pub(crate) fn take(from: &mut Vec<BoxKey>) -> Self {
        Self {
            entries: core::mem::take(from),
        }
    }

    /// Puts the compacted entries back, keeping anything registered while they were out.
    pub(crate) fn restore(mut self, into: &mut Vec<BoxKey>) {
        self.entries.append(into);
        *into = self.entries;
    }
}
