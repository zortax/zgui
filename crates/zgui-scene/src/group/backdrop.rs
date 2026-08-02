//! A filter over the composite beneath a group.

use smallvec::SmallVec;
use zgui_geom::{Device, DevicePx, Rect};

use crate::group::filter::Filter;
use crate::group::source::read_extent;
use crate::id::{ClipId, DrawOrder};

/// A `backdrop-filter`: a filter chain applied to whatever is already drawn beneath a rectangle.
///
/// It is the one primitive that *samples the destination*, which is why its read extent matters
/// more than a group's. Sampling outside a region that has been redrawn this frame reads the
/// previous frame's composite — which already contains this filter's own output, so a frosted panel
/// smears a little further every frame until the whole panel is fog.
#[derive(Clone, Debug, PartialEq)]
pub struct BackdropFilter {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// What the filter writes.
    pub bounds: Rect<DevicePx, Device>,
    /// What the filter **reads** from beneath it, which is [`BackdropFilter::bounds`] inflated by
    /// the chain's kernel support.
    ///
    /// Computed by [`read_extent`], exactly as a group boundary's is, so the two cannot disagree.
    pub source: Rect<DevicePx, Device>,
    /// The chain this draws through.
    pub clip: ClipId,
    /// The filters applied to what lies beneath.
    pub filters: SmallVec<[Filter; 2]>,
}

impl BackdropFilter {
    /// A backdrop filter over `bounds`.
    pub fn new(bounds: Rect<DevicePx, Device>, filters: SmallVec<[Filter; 2]>) -> Self {
        Self {
            order: 0,
            bounds,
            source: read_extent(bounds, &filters),
            clip: ClipId::ROOT,
            filters,
        }
    }

    /// The same filter drawn through `clip`.
    pub fn clipped(mut self, clip: ClipId) -> Self {
        self.clip = clip;
        self
    }

    /// Whether the filter reads exactly what it writes.
    ///
    /// True of every per-pixel chain — a plain `backdrop-filter: saturate(180%)` header, for
    /// instance — and those are deliberately *not* expanded for, because the pixels they read are
    /// the pixels they are already covering.
    pub fn reads_only_what_it_writes(&self) -> bool {
        self.source == self.bounds
    }
}
