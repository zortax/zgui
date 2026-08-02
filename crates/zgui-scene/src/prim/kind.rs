//! The total order primitives take at equal draw order.

/// Which kind of primitive an entry of the display list is.
///
/// The variant order is the tie-break between two primitives at the same draw order — and it is a
/// **batching preference, not a correctness mechanism**. Two primitives at equal draw order are
/// provably non-overlapping, because [`BoundsTree`](crate::BoundsTree) gives anything that overlaps
/// something else a strictly higher order; so their relative sequence cannot change a pixel, and
/// the order chosen here is the one that minimises how often a renderer has to switch pipelines.
///
/// Correct CSS painting order comes from emitting primitives in the right sequence, never from
/// this enum. A total order is kept anyway, so that a scene printed as a transcript is stable and
/// diffable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrimitiveKind {
    /// A group's opening marker, which redirects everything after it into a target of its own.
    GroupStart,
    /// A box shadow.
    Shadow,
    /// A rounded, bordered rectangle.
    Quad,
    /// A composite of rasterised vector content.
    Vector,
    /// A text decoration line.
    Decoration,
    /// A single-channel coverage sprite.
    MonoSprite,
    /// A three-channel coverage sprite.
    SubpixelSprite,
    /// A full-colour sprite.
    ColorSprite,
    /// A texture the renderer did not draw.
    External,
    /// A filter sampling the composite beneath it.
    Backdrop,
    /// A group's closing marker, which composites its target back down.
    GroupEnd,
}

impl PrimitiveKind {
    /// Every kind, in tie-break order.
    pub const ALL: [Self; 11] = [
        Self::GroupStart,
        Self::Shadow,
        Self::Quad,
        Self::Vector,
        Self::Decoration,
        Self::MonoSprite,
        Self::SubpixelSprite,
        Self::ColorSprite,
        Self::External,
        Self::Backdrop,
        Self::GroupEnd,
    ];

    /// Whether a group's start or end marker.
    ///
    /// Markers are matched pairs and are never dropped, whatever a clip or a damage set says about
    /// them: half a pair leaves a target open or composites one that was never begun.
    pub const fn is_group_marker(self) -> bool {
        matches!(self, Self::GroupStart | Self::GroupEnd)
    }
}
