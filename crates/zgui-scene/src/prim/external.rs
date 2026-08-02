//! Content the renderer did not draw: video frames, screen captures, anything with its own texture.

use zgui_geom::{Device, DevicePx, Rect};

use crate::id::{ClipId, DrawOrder};
use crate::spatial::SpatialId;

/// A texture the renderer is handed rather than one it filled.
///
/// It is opaque here on purpose: the display list is backend-neutral, and what a texture *is* —
/// which device it belongs to, what format it holds — is the renderer's knowledge. A renderer keeps
/// its own registry keyed by this handle and resolves it when the frame is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExternalTextureId(pub u64);

/// A rectangle showing an external texture.
///
/// Unlike every other primitive this is not instanced and not plain-old data: there is exactly one
/// per video or capture surface in a frame, and each is drawn on its own with its own bind group,
/// so packing them into a buffer would buy nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExternalQuad {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// Where the texture lands on the surface.
    pub bounds: Rect<DevicePx, Device>,
    /// Which texture to show.
    pub texture: ExternalTextureId,
    /// A multiplier on the texture's own alpha.
    pub opacity: f32,
    /// The chain this draws through.
    pub clip: ClipId,
    /// The transform this draws under.
    pub transform: SpatialId,
}

impl ExternalQuad {
    /// A fully opaque, unclipped quad showing `texture` in `bounds`.
    pub fn new(bounds: Rect<DevicePx, Device>, texture: ExternalTextureId) -> Self {
        Self {
            order: 0,
            bounds,
            texture,
            opacity: 1.0,
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
        }
    }

    /// The same quad drawn through `clip`.
    pub fn clipped(mut self, clip: ClipId) -> Self {
        self.clip = clip;
        self
    }

    /// The rectangle this paints.
    pub fn ink(&self) -> Rect<DevicePx, Device> {
        self.bounds
    }
}
