//! A rectangle an application's own shader draws.

use bytemuck::{Pod, Zeroable};
use zgui_geom::{Corners, Device, DevicePx, Rect, Vec2};

use crate::id::{ClipId, DrawOrder};
use crate::paint::PaintRef;
use crate::shader::{ShaderId, ShaderParamsSlot};
use crate::spatial::SpatialId;

/// A rounded rectangle whose pixels an application's shader decides.
///
/// It is a [`Quad`](crate::Quad) with four more words, and that is the point: it travels through
/// the same arena, the same draw-order permutation and the same chunk offsets, so an effect that
/// merely moved keeps its resident bytes exactly as a background does.
///
/// What the two extra pairs carry is *which* shader and *which* parameters. Neither is read by the
/// device from here — the shader decides the pipeline and the parameters are bound beside the draw
/// — but both are read on the host, because two effects with different shaders or different
/// parameters cannot be drawn by one call and the batcher has to be able to see that.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct ShadedQuad {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// Which registered effect draws it.
    pub shader: u32,
    /// The rectangle, as `[x, y, width, height]`.
    pub bounds: [f32; 4],
    /// Elliptical corner radii, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
    /// Border widths, as `[top, right, bottom, left]`.
    pub border: [f32; 4],
    /// What fills the rectangle, for an effect that shapes rather than shades.
    pub fill: PaintRef,
    /// What draws its border, for an effect that shapes rather than shades.
    pub stroke: PaintRef,
    /// The [`ClipId`] this draws through.
    pub clip: u32,
    /// The slot of the [`SpatialId`] this draws under.
    pub transform: u32,
    /// Where the space its paints are described in has its origin, as `[x, y]`.
    pub paint_origin: [f32; 2],
    /// Which parameter block it draws with.
    pub params: u32,
    /// The alpha folded in from the groups above.
    pub opacity: f32,
}

impl ShadedQuad {
    /// A square-cornered, borderless rectangle drawn by `shader` with `params`.
    pub fn new(bounds: Rect<DevicePx, Device>, shader: ShaderId, params: ShaderParamsSlot) -> Self {
        Self {
            order: 0,
            shader: shader.0,
            bounds: [
                bounds.origin.x.0,
                bounds.origin.y.0,
                bounds.size.width.0,
                bounds.size.height.0,
            ],
            radii: [0.0; 8],
            border: [0.0; 4],
            fill: PaintRef::NONE,
            stroke: PaintRef::NONE,
            clip: ClipId::ROOT.0,
            transform: SpatialId::VIEWPORT.index(),
            paint_origin: [0.0, 0.0],
            params: params.0,
            opacity: 1.0,
        }
    }

    /// The same rectangle drawn through `clip`.
    pub fn clipped(mut self, clip: ClipId) -> Self {
        self.clip = clip.0;
        self
    }

    /// The same rectangle drawn under `transform`.
    pub fn transformed(mut self, transform: SpatialId) -> Self {
        self.transform = transform.index();
        self
    }

    /// The same rectangle with elliptical corner radii.
    pub fn with_radii(mut self, radii: Corners<Vec2<DevicePx>>) -> Self {
        self.radii = [
            radii.top_left.x.0,
            radii.top_left.y.0,
            radii.top_right.x.0,
            radii.top_right.y.0,
            radii.bottom_right.x.0,
            radii.bottom_right.y.0,
            radii.bottom_left.x.0,
            radii.bottom_left.y.0,
        ];
        self
    }

    /// The same rectangle with a border of the given widths and paint.
    ///
    /// Read only by a coverage effect: a paint effect shades the whole box itself and is handed no
    /// border to draw.
    pub fn with_border(mut self, widths: [f32; 4], stroke: PaintRef) -> Self {
        self.border = widths;
        self.stroke = stroke;
        self
    }

    /// The same rectangle filled with `fill` where its shape covers.
    pub fn with_fill(mut self, fill: PaintRef) -> Self {
        self.fill = fill;
        self
    }

    /// The same rectangle at `opacity`, folded in from the groups above.
    pub fn faded(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Moves the space this rectangle's paints are described in by `by`.
    pub fn reanchor_paint(&mut self, by: zgui_geom::Size<DevicePx, Device>) {
        self.paint_origin[0] += by.width.0;
        self.paint_origin[1] += by.height.0;
    }

    /// Whether either paint is read at the point being drawn rather than being one colour.
    ///
    /// A flat fill is the same colour everywhere, so where it is sampled cannot matter; a ramp and
    /// a sampled image are both read at a point, so both stop agreeing with their rectangle the
    /// moment it moves without them.
    pub fn samples_its_paint(&self) -> bool {
        [self.fill, self.stroke].into_iter().any(|paint| {
            paint.kind == crate::paint::PaintKind::Gradient as u32
                || paint.kind == crate::paint::PaintKind::Image as u32
        })
    }

    /// Which effect draws this.
    pub fn shader_id(&self) -> ShaderId {
        ShaderId(self.shader)
    }

    /// Which parameter block it draws with.
    pub fn params_slot(&self) -> ShaderParamsSlot {
        ShaderParamsSlot(self.params)
    }

    /// The rectangle this paints, which is what draw order and culling are computed from.
    pub fn ink(&self) -> Rect<DevicePx, Device> {
        crate::prim::layout::rect_of(self.bounds)
    }

    /// The clip chain this draws through.
    pub fn clip_id(&self) -> ClipId {
        ClipId(self.clip)
    }
}
