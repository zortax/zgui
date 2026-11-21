//! The reusable buffers one composition walk works in.
//!
//! Every visited box used to allocate its own child list copy, ink list, kind list and written
//! list, which put four heap allocations inside the hottest recursive walk the engine runs on
//! every frame. The walk now appends into these buffers behind a mark and truncates back to it on
//! the way out, so recursion nests regions instead of allocating, and a warm walk allocates
//! nothing at all.
//!
//! A region is valid for exactly one visit: a deeper visit appends past the caller's region and
//! restores its own mark before returning, so indices a caller holds stay meaningful across
//! recursion. Nothing here owns anything — fragment keys copied in are validated against the
//! store when they are read back.

use rustc_hash::FxHashMap;
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_scene::ClipId;

use crate::fragment::{FragKey, FragmentKind};

/// One clip chain's resolved bounds, under the spatial tree's write stamp it was resolved at.
#[derive(Clone, Copy, Debug)]
pub(super) struct KeptAdmitted {
    /// [`PropertyTree::writes`](zgui_scene::PropertyTree::writes) when `bounds` was resolved.
    pub(super) stamp: u64,
    /// What [`ClipTable::bounds_placed`](zgui_scene::ClipTable::bounds_placed) gave.
    pub(super) bounds: Rect<DevicePx, Device>,
    /// [`Table::version`](zgui_scene::Table::version) of the clip table when `bounds` was last
    /// verified against an uncached resolution.
    #[cfg(debug_assertions)]
    pub(super) verified_at: zgui_scene::TableVersion,
}

/// The buffers, owned by whoever runs composition walks and lent to each one.
#[derive(Debug, Default)]
pub struct DiffScratch {
    /// The children of the box being visited, copied so the store borrow can end.
    pub(super) children: Vec<BoxKey>,
    /// The ink of a box's later own pieces and of each child subtree.
    pub(super) child_inks: Vec<Rect<DevicePx, Device>>,
    /// What each fragment of the box being visited draws.
    pub(super) kinds: Vec<FragmentKind>,
    /// The fragments written for boxes on the current descent path.
    pub(super) written: Vec<FragKey>,
    /// The fragments a box is about to retire.
    pub(super) stale: Vec<FragKey>,
    /// Resolved device-space clip bounds kept within the active pass.
    pub(super) admitted: FxHashMap<ClipId, KeptAdmitted>,
    /// The boxes that imposed a clip and left the tree, until the frame's owner takes them.
    pub(super) retired_clips: Vec<zgui_scene::PropertyOwner>,
}

impl DiffScratch {
    /// Takes the boxes that imposed a clip and left the tree since the last call.
    pub fn take_retired_clips(&mut self) -> impl Iterator<Item = zgui_scene::PropertyOwner> + '_ {
        self.retired_clips.drain(..)
    }
}
