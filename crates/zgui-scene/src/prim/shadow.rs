//! Box shadows, drop and inset.

use bytemuck::{Pod, Zeroable};
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Rect};

use crate::id::{ClipId, DrawOrder};
use crate::prim::layout::rect_of;
use crate::spatial::SpatialId;

/// A blurred rounded rectangle, cast by a box.
///
/// One struct serves both `box-shadow` forms. The blurred shape is `shape_bounds` with `radii`. A
/// drop shadow paints that shape outside the box that cast it, so its `bounds` is the shape dilated
/// by the blur. An inset shadow paints the complement of that shape inside the box, so its `bounds`
/// is the box itself. Either way `bounds` is what the primitive paints, which is what draw order
/// and culling are computed from.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Shadow {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// The blur's standard deviation, in device pixels.
    pub blur: f32,
    /// Everything this paints, as `[x, y, width, height]`.
    pub bounds: [f32; 4],
    /// The shape the blur is applied to, as `[x, y, width, height]`.
    ///
    /// It is the casting box offset and grown by the spread for a drop shadow, and the hole the
    /// shadow leaves open for an inset one: the box offset and shrunk by the spread.
    pub shape_bounds: [f32; 4],
    /// The shadow shape's elliptical corner radii, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
    /// The casting box, as `[x, y, width, height]`.
    pub element_bounds: [f32; 4],
    /// The casting box's elliptical corner radii.
    pub element_radii: [f32; 8],
    /// Premultiplied, gamma-encoded sRGB.
    pub color: [f32; 4],
    /// The [`ClipId`] this draws through.
    pub clip: u32,
    /// The slot of the [`SpatialId`] this draws under.
    pub transform: u32,
    /// One when the shadow is inset, zero when it is cast outwards.
    pub inset: u32,
    /// The superellipse exponent the element's corners are cut with; two is the ellipse.
    ///
    /// A shadow is the element's own shape blurred, so it has to be cut the same way: a squircle
    /// casting a rounded-rectangle shadow shows the shadow's corners outside its own.
    ///
    /// This was the padding word the structure needed to be copied as bytes, which is why adding
    /// it costs a shadow nothing.
    pub shape: f32,
}

impl Shadow {
    /// How many standard deviations of blur a shadow visibly reaches.
    ///
    /// Three is where a Gaussian falls below one part in a thousand, which is under half a level at
    /// eight bits per channel: dilating by less leaves a visible edge where the shadow is cut off,
    /// and dilating by more costs pixels that cannot be seen.
    pub const BLUR_EXTENT: f32 = 3.0;

    /// A shadow cast outwards from `element`, blurred by `blur` standard deviations.
    ///
    /// The painted extent is derived here rather than taken from the caller, so that the rectangle
    /// culling and ordering use is the rectangle the shader actually covers.
    pub fn drop_shadow(
        element: Rect<DevicePx, Device>,
        offset: (f32, f32),
        spread: f32,
        blur: f32,
        color: Color,
    ) -> Self {
        let shape = [
            element.origin.x.0 + offset.0 - spread,
            element.origin.y.0 + offset.1 - spread,
            element.size.width.0 + 2.0 * spread,
            element.size.height.0 + 2.0 * spread,
        ];
        let reach = Self::BLUR_EXTENT * blur;
        Self {
            order: 0,
            blur,
            bounds: [
                shape[0] - reach,
                shape[1] - reach,
                shape[2] + 2.0 * reach,
                shape[3] + 2.0 * reach,
            ],
            shape_bounds: shape,
            radii: [0.0; 8],
            element_bounds: [
                element.origin.x.0,
                element.origin.y.0,
                element.size.width.0,
                element.size.height.0,
            ],
            element_radii: [0.0; 8],
            color: color.to_premultiplied_srgb(),
            clip: ClipId::ROOT.0,
            transform: SpatialId::VIEWPORT.index(),
            inset: 0,
            shape: crate::prim::CornerShape::ROUND.get(),
        }
    }

    /// A shadow cast inwards, which paints only inside `element`.
    ///
    /// `element` is the box the shadow is clipped to, which is the padding box in CSS. The hole is
    /// that box offset and shrunk by `spread`. A spread past the box's half extent closes the hole,
    /// and the shadow then covers the whole box.
    pub fn inset_shadow(
        element: Rect<DevicePx, Device>,
        offset: (f32, f32),
        spread: f32,
        blur: f32,
        color: Color,
    ) -> Self {
        let mut shadow = Self::drop_shadow(element, offset, 0.0, blur, color);
        let width = (element.size.width.0 - 2.0 * spread).max(0.0);
        let height = (element.size.height.0 - 2.0 * spread).max(0.0);
        let center_x = element.origin.x.0 + offset.0 + element.size.width.0 * 0.5;
        let center_y = element.origin.y.0 + offset.1 + element.size.height.0 * 0.5;
        shadow.shape_bounds = [
            center_x - width * 0.5,
            center_y - height * 0.5,
            width,
            height,
        ];
        shadow.inset = 1;
        shadow.bounds = shadow.element_bounds;
        shadow
    }

    /// The same shadow cast by an element whose corners are cut to `shape`.
    pub fn with_corner_shape(mut self, shape: crate::prim::CornerShape) -> Self {
        self.shape = shape.get();
        self
    }

    /// The same shadow drawn through `clip`.
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
