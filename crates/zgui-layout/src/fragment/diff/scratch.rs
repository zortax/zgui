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

use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Rect};

use crate::fragment::{FragKey, FragmentKind};

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
}
