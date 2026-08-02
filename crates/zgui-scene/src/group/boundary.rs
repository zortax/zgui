//! The marker pair that makes a stacking context composite as a unit.

use smallvec::SmallVec;
use zgui_geom::{Device, DevicePx, Rect};

use crate::group::filter::Filter;
use crate::group::source::read_extent;
use crate::id::{ClipId, DrawOrder};
use crate::spatial::SpatialId;

/// A start or end marker for content that must be drawn into a target of its own and composited
/// once.
///
/// A group is what makes `opacity`, `mix-blend-mode`, `isolation` and `filter` composite the way
/// CSS says they do: without one, two overlapping children under a half-transparent parent are each
/// blended separately and the overlap darkens twice.
///
/// Markers are **matched pairs and are never dropped** — not by a clip that admits nothing, not by
/// a damage set that misses them. Half a pair leaves a target open or composites one that was never
/// begun.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupBoundary {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// What the group writes.
    pub bounds: Rect<DevicePx, Device>,
    /// What the group **reads**, which is [`GroupBoundary::bounds`] inflated by the filter chain's
    /// kernel support.
    ///
    /// Equal to `bounds` for the overwhelming majority of groups — every per-pixel filter, every
    /// blend mode, and plain opacity — and larger only where a blur or a drop shadow samples
    /// outside what it writes. It is computed by [`read_extent`], the same function every other
    /// reader of this quantity calls, so no two of them can disagree about it.
    ///
    /// It is deliberately not folded into the ink of what the group contains: that would inflate
    /// damage for the many fragments that read nothing.
    pub source: Rect<DevicePx, Device>,
    /// The chain this draws through.
    pub clip: ClipId,
    /// A multiplier on the whole group's alpha.
    pub opacity: f32,
    /// How the group composites onto what is beneath it.
    pub blend: peniko::BlendMode,
    /// The filters applied to the group's own content.
    pub filters: SmallVec<[Filter; 2]>,
    /// The transform the group composites under.
    pub transform: Option<SpatialId>,
    /// Whether this is the opening marker.
    pub is_start: bool,
}

impl GroupBoundary {
    /// An opening marker for `bounds`, with the given opacity, blend mode and filters.
    ///
    /// The read extent is derived rather than accepted, so a caller cannot supply one that
    /// disagrees with the filters beside it.
    pub fn start(
        bounds: Rect<DevicePx, Device>,
        opacity: f32,
        blend: peniko::BlendMode,
        filters: SmallVec<[Filter; 2]>,
    ) -> Self {
        Self {
            order: 0,
            bounds,
            source: read_extent(bounds, &filters),
            clip: ClipId::ROOT,
            opacity,
            blend,
            filters,
            transform: None,
            is_start: true,
        }
    }

    /// The closing marker matching this one.
    pub fn end(&self) -> Self {
        Self {
            is_start: false,
            ..self.clone()
        }
    }

    /// The same marker drawn through `clip`.
    pub fn clipped(mut self, clip: ClipId) -> Self {
        self.clip = clip;
        self
    }

    /// Whether the group reads exactly what it writes.
    ///
    /// True for nearly every group. A caller expanding damage for composites that read outside
    /// themselves tests this and skips the rest.
    pub fn reads_only_what_it_writes(&self) -> bool {
        self.source == self.bounds
    }
}
