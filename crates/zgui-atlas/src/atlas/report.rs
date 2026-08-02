//! What the atlas is currently holding.

/// A snapshot of the atlas's occupancy, for budgeting and for diagnostics.
///
/// Every field is a count of something a growing document makes grow, so a budget assertion can be
/// written against it and a leak shows up as a number that never comes back down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtlasReport {
    /// How many rasters are cached.
    pub tiles: usize,
    /// How many of those are held against eviction by at least one caller.
    pub referenced_tiles: usize,
    /// How many bytes those held rasters occupy, weighted by each one's own format.
    ///
    /// Beside the count rather than instead of it, because the two answer different questions: the
    /// count says how much is being held, this says how much of the budget holding it is spending.
    /// A thousand held glyphs and one held picture can be the same number of bytes.
    ///
    /// It is the tiles' own bytes and not a share of the textures they sit in. Texture memory comes
    /// back only when a whole texture empties, so a held tile does not keep exactly this much
    /// resident — it keeps whatever texture it is in alive. This is the lower bound, and it is the
    /// one figure that follows from the tiles alone.
    pub referenced_bytes: u64,
    /// How many textures exist across every pool.
    pub textures: usize,
    /// How many texels those textures hold between them.
    ///
    /// Texels rather than bytes, because a pool's format decides the byte cost and two pools have
    /// different ones; [`AtlasReport::bytes`] is the weighted total.
    pub texels: u64,
    /// How many bytes those textures occupy, weighted by each pool's format.
    pub bytes: u64,
    /// How many uploads are queued and not yet flushed.
    pub pending_uploads: usize,
    /// How many bytes those queued uploads hold.
    pub pending_bytes: u64,
}
