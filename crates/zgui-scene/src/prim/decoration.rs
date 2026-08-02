//! Text decoration lines: underline, overline and strikethrough.

use bytemuck::{Pod, Zeroable};
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Rect};

use crate::id::{ClipId, DrawOrder};
use crate::prim::layout::rect_of;
use crate::spatial::SpatialId;

/// How a decoration line is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum DecorationStyle {
    /// One continuous line.
    Solid = 0,
    /// A repeating wave.
    Wavy = 1,
    /// Dashes with gaps between them.
    Dashed = 2,
    /// Dots with gaps between them.
    Dotted = 3,
    /// Two parallel lines.
    Double = 4,
}

/// A decoration line under, over or through a run of text.
///
/// All three of `text-decoration-line`'s values are this one primitive: they differ only in where
/// the rectangle sits, which the caller has already decided.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Decoration {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// The [`DecorationStyle`] discriminant.
    pub style: u32,
    /// The rectangle the line occupies, as `[x, y, width, height]`.
    pub bounds: [f32; 4],
    /// Premultiplied, gamma-encoded sRGB.
    pub color: [f32; 4],
    /// The line's thickness, in device pixels.
    pub thickness: f32,
    /// The [`ClipId`] this draws through.
    pub clip: u32,
    /// The slot of the [`SpatialId`] this draws under.
    pub transform: u32,
    /// Written zero. Present so the struct has no padding and can be copied as bytes.
    pub reserved: u32,
}

impl Decoration {
    /// A line filling `bounds`, in the given colour and style.
    pub fn new(
        bounds: Rect<DevicePx, Device>,
        thickness: f32,
        color: Color,
        style: DecorationStyle,
    ) -> Self {
        Self {
            order: 0,
            style: style as u32,
            bounds: [
                bounds.origin.x.0,
                bounds.origin.y.0,
                bounds.size.width.0,
                bounds.size.height.0,
            ],
            color: color.to_premultiplied_srgb(),
            thickness,
            clip: ClipId::ROOT.0,
            transform: SpatialId::VIEWPORT.index(),
            reserved: 0,
        }
    }

    /// The same line drawn through `clip`.
    pub fn clipped(mut self, clip: ClipId) -> Self {
        self.clip = clip.0;
        self
    }

    /// The rectangle this paints.
    pub fn ink(&self) -> Rect<DevicePx, Device> {
        rect_of(self.bounds)
    }

    /// The clip chain this draws through.
    pub fn clip_id(&self) -> ClipId {
        ClipId(self.clip)
    }
}
