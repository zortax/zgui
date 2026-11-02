//! What one pass of the separable blur is told.

use bytemuck::{Pod, Zeroable};
use zgui_geom::{Device, Point, Rect, Size};

use crate::target::scale::TargetScale;

/// One axis of a separable gaussian, or the downsample that precedes the pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlurAxis {
    /// Along x.
    Horizontal,
    /// Along y.
    Vertical,
}

impl BlurAxis {
    /// The direction, in texels.
    fn direction(self) -> [f32; 2] {
        match self {
            Self::Horizontal => [1.0, 0.0],
            Self::Vertical => [0.0, 1.0],
        }
    }
}

/// The block one blur pass reads.
///
/// Every extent and every scale is explicit rather than derived from the frame's viewport, which
/// is what lets the chain run over a sub-rectangle: a pass anchored to the window would re-anchor
/// its sampling lattice whenever the blurred content moved, and the halo would then shift by about
/// a pixel per frame under animation — the exact artefact the snapped grid exists to prevent.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct BlurParams {
    /// The source extent in texels, then the destination's.
    pub extents: [f32; 4],
    /// The direction in source texels, the deviation in source texels, and the taps per side.
    pub kernel: [f32; 4],
    /// Source texels per device pixel, destination texels per device pixel, the tap spacing, and
    /// whether this pass halves the resolution.
    pub sampling: [f32; 4],
    /// The corners of the part of the source that was written, in source texels.
    pub valid: [f32; 4],
    /// The device pixel the source's first texel holds, then the same for the destination.
    pub origins: [f32; 4],
}

impl BlurParams {
    /// How many taps one axis takes on each side of the centre.
    ///
    /// A gaussian is negligible beyond three deviations, so a kernel wide enough for that is a
    /// complete blur rather than a truncated one. Beyond this count the taps *spread out* instead
    /// of the kernel being cut short — a slightly under-sampled gaussian rather than a hard ring
    /// where the kernel was chopped.
    pub const MAX_TAPS: f32 = 16.0;

    /// The parameters for the 2:1 downsample that begins the chain.
    ///
    /// `source` and `destination` are the two targets' extents in texels and the device pixels
    /// their first texels hold.
    pub fn downsample(
        source: (Size<i32, Device>, Point<i32, Device>),
        source_scale: TargetScale,
        destination: (Size<i32, Device>, Point<i32, Device>),
        written: Rect<i32, Device>,
    ) -> Self {
        Self {
            extents: extents(source.0, destination.0),
            kernel: [0.0, 0.0, 0.0, 0.0],
            sampling: [source_scale.factor(), TargetScale::Half.factor(), 1.0, 1.0],
            valid: valid(written, source_scale, source.1),
            origins: origins(source.1, destination.1),
        }
    }

    /// The parameters for one axis of the gaussian at `deviation` device pixels.
    ///
    /// The deviation is converted into the source's own texels here, because the pair of axis
    /// passes runs at half resolution and a deviation left in device pixels would blur twice as
    /// far as the content asked for.
    ///
    /// Both targets are held at `scale`. `extents` are the source's and the destination's extents
    /// in texels, and `origins` are the device pixels their first texels hold.
    pub fn axis(
        extents: [Size<i32, Device>; 2],
        scale: TargetScale,
        origins: [Point<i32, Device>; 2],
        axis: BlurAxis,
        deviation: f32,
        written: Rect<i32, Device>,
    ) -> Self {
        let texels = (deviation * scale.factor()).max(0.0);
        let reach = 3.0 * texels;
        let spacing = (reach / Self::MAX_TAPS).max(1.0);
        let taps = (reach / spacing).ceil().clamp(1.0, Self::MAX_TAPS);
        let direction = axis.direction();
        Self {
            extents: self::extents(extents[0], extents[1]),
            kernel: [direction[0], direction[1], texels, taps],
            sampling: [scale.factor(), scale.factor(), spacing, 0.0],
            valid: valid(written, scale, origins[0]),
            origins: self::origins(origins[0], origins[1]),
        }
    }

    /// How far this pass reaches, in source texels.
    pub fn reach(&self) -> f32 {
        self.kernel[3] * self.sampling[2]
    }
}

/// A device-pixel region in a target's own texels, as the block holds it.
fn valid(written: Rect<i32, Device>, scale: TargetScale, origin: Point<i32, Device>) -> [f32; 4] {
    let texels = scale.texels_at(written, origin);
    [
        texels.left() as f32,
        texels.top() as f32,
        texels.right() as f32,
        texels.bottom() as f32,
    ]
}

/// The source and destination origins, as the block holds them.
fn origins(source: Point<i32, Device>, destination: Point<i32, Device>) -> [f32; 4] {
    [
        source.x as f32,
        source.y as f32,
        destination.x as f32,
        destination.y as f32,
    ]
}

/// The source and destination extents, as the block holds them.
fn extents(source: Size<i32, Device>, destination: Size<i32, Device>) -> [f32; 4] {
    [
        source.width.max(1) as f32,
        source.height.max(1) as f32,
        destination.width.max(1) as f32,
        destination.height.max(1) as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::{BlurAxis, BlurParams};
    use crate::target::scale::TargetScale;
    use zgui_geom::{Device, Point, Rect, Size};

    /// A region covering the whole of a test extent.
    fn everywhere() -> Rect<i32, Device> {
        Rect::new(Point::new(0, 0), Size::new(64, 64))
    }

    #[test]
    fn a_deviation_in_device_pixels_becomes_one_in_the_texels_the_pass_runs_at() {
        let params = BlurParams::axis(
            [Size::new(64, 64); 2],
            TargetScale::Half,
            [Point::ORIGIN; 2],
            BlurAxis::Horizontal,
            8.0,
            everywhere(),
        );
        assert_eq!(
            params.kernel[2], 4.0,
            "half resolution halves the deviation"
        );
    }

    #[test]
    fn a_wide_radius_spreads_its_taps_rather_than_cutting_the_kernel_short() {
        let narrow = BlurParams::axis(
            [Size::new(64, 64); 2],
            TargetScale::Full,
            [Point::ORIGIN; 2],
            BlurAxis::Vertical,
            2.0,
            everywhere(),
        );
        assert_eq!(narrow.sampling[2], 1.0, "a small kernel needs no spreading");
        assert!(narrow.reach() >= 3.0 * 2.0, "three deviations are covered");

        let wide = BlurParams::axis(
            [Size::new(64, 64); 2],
            TargetScale::Full,
            [Point::ORIGIN; 2],
            BlurAxis::Vertical,
            40.0,
            everywhere(),
        );
        assert!(wide.kernel[3] <= BlurParams::MAX_TAPS);
        assert!(wide.sampling[2] > 1.0, "the taps spread out");
        assert!(
            wide.reach() >= 3.0 * 40.0 - wide.sampling[2],
            "the kernel still spans three deviations: {} against {}",
            wide.reach(),
            3.0 * 40.0
        );
    }

    #[test]
    fn the_downsample_halves_the_destination_and_says_which_grid_it_wrote() {
        let params = BlurParams::downsample(
            (Size::new(64, 64), Point::ORIGIN),
            TargetScale::Full,
            (Size::new(32, 32), Point::ORIGIN),
            everywhere(),
        );
        assert_eq!(params.extents, [64.0, 64.0, 32.0, 32.0]);
        assert_eq!(params.sampling[0], 1.0);
        assert_eq!(params.sampling[1], 0.5);
        assert_eq!(params.origins, [0.0; 4]);
    }

    #[test]
    fn what_was_written_is_measured_from_the_source_origin() {
        let written = Rect::new(Point::new(10, 20), Size::new(30, 40));
        let downsample = BlurParams::downsample(
            (Size::new(64, 64), Point::new(4, 8)),
            TargetScale::Full,
            (Size::new(32, 32), Point::new(2, 6)),
            written,
        );
        assert_eq!(downsample.valid, [6.0, 12.0, 36.0, 52.0]);
        assert_eq!(downsample.origins, [4.0, 8.0, 2.0, 6.0]);

        let axis = BlurParams::axis(
            [Size::new(32, 32), Size::new(48, 40)],
            TargetScale::Half,
            [Point::new(2, 6), Point::new(0, 0)],
            BlurAxis::Vertical,
            4.0,
            written,
        );
        assert_eq!(axis.valid, [4.0, 7.0, 19.0, 27.0]);
        assert_eq!(
            axis.extents,
            [32.0, 32.0, 48.0, 40.0],
            "the two targets of a pass can be of different size classes"
        );
        assert_eq!(axis.origins, [2.0, 6.0, 0.0, 0.0]);
    }
}
