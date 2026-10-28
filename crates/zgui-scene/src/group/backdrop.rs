//! A filter over the composite beneath a group.

use smallvec::SmallVec;
use zgui_geom::{Device, DevicePx, Rect};

use crate::group::filter::Filter;
use crate::group::source::read_extent;
use crate::id::{ClipId, DrawOrder};
use crate::spatial::SpatialId;

/// Where a backdrop filter's copy of what lies beneath comes from.
///
/// A backdrop cannot read the target it is writing, so what it filters is always a copy. The
/// question this answers is whether the copy has to be made afresh from the composite, and it is
/// the difference between a frosted panel costing its own area every frame and costing whatever
/// changed under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackdropCapture {
    /// Copy everything the filter reads out of the composite, this frame.
    ///
    /// The copy is then this frame's work throughout, so every pixel of it has to be a pixel this
    /// frame redrew — which is what makes the damage set grow to cover the whole read region.
    #[default]
    Renewed,
    /// Copy only what this frame redrew into the copy kept from the frame before.
    ///
    /// Legal exactly when nothing else can have changed the rest of it: outside the damage the
    /// kept copy holds an *earlier* frame's composite at pixels no frame since has redrawn, which
    /// is the same picture and not the fogged one the composed target holds there.
    Kept,
}

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
    /// What the filter writes, in the coordinates of [`BackdropFilter::transform`].
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
    /// The coordinate system the rectangles are measured in, or `None` for the device.
    ///
    /// The renderer maps the rectangles onto the device through the system's matrix in the frame
    /// it draws, so a place write that moves the system moves the filter with the content.
    pub transform: Option<SpatialId>,
    /// Whether the copy this filters may be the one kept from the last frame.
    ///
    /// Decided where the damage is grown and recorded here, because the two are one decision: a
    /// kept copy is what lets the damage stay small, and a damage set that stayed small without
    /// one would leave the filter reading pixels no frame has written.
    pub capture: BackdropCapture,
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
            transform: None,
            capture: BackdropCapture::Renewed,
        }
    }

    /// The same filter measured in the coordinate system `space`.
    pub fn in_space(mut self, space: Option<SpatialId>) -> Self {
        self.transform = space;
        self
    }

    /// The same filter drawn through `clip`.
    pub fn clipped(mut self, clip: ClipId) -> Self {
        self.clip = clip;
        self
    }

    /// The same filter reading the copy kept from the last frame.
    pub fn keeping_its_capture(mut self) -> Self {
        self.capture = BackdropCapture::Kept;
        self
    }

    /// Whether this reads the copy kept from the last frame.
    pub fn keeps_its_capture(&self) -> bool {
        self.capture == BackdropCapture::Kept
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
