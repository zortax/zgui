//! What a renderer is holding in reusable target pools.

/// The reusable render targets a renderer is holding, as a budget reads them.
///
/// A renderer's targets divide into two kinds and only one of them is a cache. The target a frame
/// is composed into is a live resource: it is exactly the size of the surface, the next frame needs
/// it, and freeing it frees nothing for longer than it takes to allocate it again. The targets
/// isolated content is composed in are the cache — how many exist follows from how deeply the
/// document nests filters and stacking contexts, they outlive the frame that asked for one, and a
/// document that has stopped isolating anything keeps every one it ever needed.
///
/// This describes the second kind alone. A renderer that pools nothing reports
/// [`TargetPoolReport::EMPTY`], which is the honest answer and not a stub: nothing it holds would
/// come back if it were asked to free.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TargetPoolReport {
    /// How many bytes of pooled targets exist right now.
    pub resident: u64,
    /// How many bytes of those are lent out and may not be freed.
    ///
    /// Non-zero only while a frame is mid-composite; between frames every lease has been returned,
    /// which is why a budget enforced at the end of a frame can free the whole pool.
    pub lent: u64,
    /// How many leases have been taken since the renderer was built.
    ///
    /// Monotonic and never reset: two readings subtracted say whether the document isolated
    /// anything between two moments, which is what distinguishes a pool that is still being drawn
    /// from from one that is merely still allocated.
    pub leases: u64,
}

impl TargetPoolReport {
    /// A renderer holding no pooled targets.
    pub const EMPTY: Self = Self {
        resident: 0,
        lent: 0,
        leases: 0,
    };
}
