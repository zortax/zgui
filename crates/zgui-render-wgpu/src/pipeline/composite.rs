//! What one composite of an isolated target is told.

use bytemuck::{Pod, Zeroable};
use zgui_geom::{Device, DevicePx, Rect, Size};

use crate::filter::matrix::ColorMatrix;
use crate::target::scale::TargetScale;

/// The block one composite reads.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct CompositeParams {
    /// The quad, in device pixels: origin then extent.
    pub bounds: [f32; 4],
    /// The sampled extent in texels, the texels one device pixel covers, and the opacity.
    pub source: [f32; 4],
    /// The clip chain, the flags, and how far the sampled content is displaced.
    pub control: [f32; 4],
    /// A shadow's colour, premultiplied.
    pub tint: [f32; 4],
    /// The filter matrix's first column.
    pub matrix0: [f32; 4],
    /// Its second column.
    pub matrix1: [f32; 4],
    /// Its third column.
    pub matrix2: [f32; 4],
    /// Its fourth column.
    pub matrix3: [f32; 4],
    /// Its constant term.
    pub matrix_offset: [f32; 4],
}

impl CompositeParams {
    /// Replace what was sampled with a flat colour scaled by its alpha, which is a drop shadow.
    pub const TINT: u32 = 1;
    /// Run the sampled colour through the filter matrix.
    pub const MATRIX: u32 = 2;

    /// A composite of `bounds`, sampling a target of `extent` texels held at `scale`.
    pub fn new(
        bounds: Rect<DevicePx, Device>,
        extent: Size<i32, Device>,
        scale: TargetScale,
        clip: u32,
    ) -> Self {
        let identity = ColorMatrix::identity().columns();
        Self {
            bounds: [
                bounds.origin.x.0,
                bounds.origin.y.0,
                bounds.size.width.0,
                bounds.size.height.0,
            ],
            source: [
                extent.width.max(1) as f32,
                extent.height.max(1) as f32,
                scale.factor(),
                1.0,
            ],
            control: [clip as f32, 0.0, 0.0, 0.0],
            tint: [0.0; 4],
            matrix0: identity[0],
            matrix1: identity[1],
            matrix2: identity[2],
            matrix3: identity[3],
            matrix_offset: identity[4],
        }
    }

    /// The same composite at `opacity`.
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.source[3] = opacity.clamp(0.0, 1.0);
        self
    }

    /// The same composite with `matrix` applied to what it samples.
    ///
    /// An identity matrix is not flagged, so a group with no per-pixel filter costs no per-fragment
    /// unpremultiply — which is the overwhelming majority of groups.
    pub fn with_matrix(mut self, matrix: ColorMatrix) -> Self {
        let columns = matrix.columns();
        self.matrix0 = columns[0];
        self.matrix1 = columns[1];
        self.matrix2 = columns[2];
        self.matrix3 = columns[3];
        self.matrix_offset = columns[4];
        if !matrix.is_identity() {
            self.control[1] = (self.flags() | Self::MATRIX) as f32;
        }
        self
    }

    /// The same composite drawn as a shadow of what it samples: `color`, displaced by `offset`.
    pub fn as_shadow(mut self, color: [f32; 4], offset: (f32, f32)) -> Self {
        self.tint = color;
        self.control[1] = (self.flags() | Self::TINT) as f32;
        self.control[2] = offset.0;
        self.control[3] = offset.1;
        self
    }

    /// The flags currently set.
    pub fn flags(&self) -> u32 {
        self.control[1] as u32
    }
}

#[cfg(test)]
mod tests {
    use super::CompositeParams;
    use crate::filter::matrix::ColorMatrix;
    use crate::target::scale::TargetScale;
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    fn bounds() -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(4.0), DevicePx(8.0)),
            Size::new(DevicePx(16.0), DevicePx(32.0)),
        )
    }

    #[test]
    fn a_group_with_no_per_pixel_filter_does_not_ask_for_the_matrix() {
        let params = CompositeParams::new(bounds(), Size::new(64, 64), TargetScale::Full, 0)
            .with_matrix(ColorMatrix::identity());
        assert_eq!(params.flags() & CompositeParams::MATRIX, 0);
        assert_eq!(params.matrix0, [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_real_filter_asks_for_the_matrix_and_carries_its_columns() {
        let params = CompositeParams::new(bounds(), Size::new(64, 64), TargetScale::Full, 0)
            .with_matrix(ColorMatrix::invert(1.0));
        assert_ne!(params.flags() & CompositeParams::MATRIX, 0);
        assert_eq!(params.matrix_offset, [1.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn a_shadow_carries_its_colour_and_its_displacement() {
        let params = CompositeParams::new(bounds(), Size::new(64, 64), TargetScale::Half, 3)
            .as_shadow([0.0, 0.0, 0.0, 0.5], (4.0, -2.0));
        assert_ne!(params.flags() & CompositeParams::TINT, 0);
        assert_eq!(params.control[2], 4.0);
        assert_eq!(params.control[3], -2.0);
        assert_eq!(
            params.source[2], 0.5,
            "a half-resolution target is magnified"
        );
        assert_eq!(params.control[0], 3.0, "the clip chain reaches the shader");
    }
}
