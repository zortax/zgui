//! The block every pipeline reads once per frame.

use bytemuck::{Pod, Zeroable};
use zgui_geom::{Device, Size};
use zgui_scene::FrameClock;

use crate::target::scale::TargetScale;

/// The display gamma coverage is corrected against.
///
/// It is a property of the display rather than of the content, and 2.2 is the sRGB display's
/// nominal value. Correcting for it is what stops light text on a dark background looking thin.
pub const DISPLAY_GAMMA: f32 = 2.2;

/// How much contrast single-channel coverage is enhanced by.
pub const GRAYSCALE_CONTRAST: f32 = 0.5;

/// How much contrast per-channel coverage is enhanced by.
///
/// Larger than the single-channel figure because per-channel coverage is sampled a third of a
/// pixel at a time and loses more of a thin stroke's weight.
pub const SUBPIXEL_CONTRAST: f32 = 1.0;

/// What every pipeline needs to know about the target it is drawing into.
///
/// It is per *target* rather than per frame, because a frame draws into more than one: the
/// persistent composed target, and a pool target for every isolated group. Every primitive is
/// positioned and clipped in device pixels whichever of those it lands in, so the mapping between
/// the two is exactly what this carries — and it is why the block is read through a dynamic
/// offset rather than written once. A single rewritten buffer would give every pass of a frame the
/// last mapping written, which is the same last-write-wins hazard that makes per-draw uniforms a
/// slot allocation everywhere else.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Globals {
    /// The target's extent in texels, then how many texels one device pixel covers on each axis.
    pub viewport: [f32; 4],
    /// The four coefficients coverage is corrected with.
    pub gamma_ratios: [f32; 4],
    /// Single-channel contrast, per-channel contrast, subpixel order, then one unused lane.
    pub text: [f32; 4],
    /// Seconds since the document started, the previous frame's duration, the scale factor, then
    /// one unused lane.
    ///
    /// Read by application effects and by nothing the framework draws. It is a lane of the block
    /// every pipeline already binds rather than a block of its own, because an effect is drawn in
    /// the same pass as everything around it and a second uniform would be a second dynamic offset
    /// on every draw that never reads it.
    pub frame: [f32; 4],
}

impl Globals {
    /// The globals for a full-resolution target of `size` device pixels.
    pub fn new(size: Size<i32, Device>, order: SubpixelOrder) -> Self {
        Self::for_target(size, TargetScale::Full, order)
    }

    /// The globals for a target covering `size` device pixels at `scale`.
    ///
    /// `size` is always the region in **device** pixels; the texel extent is derived, so a caller
    /// cannot hand in an extent that disagrees with the scale beside it.
    pub fn for_target(size: Size<i32, Device>, scale: TargetScale, order: SubpixelOrder) -> Self {
        let extent = scale.extent(size);
        Self {
            viewport: [
                extent.width.max(1) as f32,
                extent.height.max(1) as f32,
                scale.factor(),
                scale.factor(),
            ],
            gamma_ratios: zgui_color::gamma_correction_ratios(DISPLAY_GAMMA),
            text: [
                GRAYSCALE_CONTRAST,
                SUBPIXEL_CONTRAST,
                f32::from(order == SubpixelOrder::BlueToRed),
                0.0,
            ],
            frame: [0.0; 4],
        }
    }

    /// The same block, telling application effects what frame they are drawing in.
    pub fn with_frame(mut self, clock: FrameClock) -> Self {
        self.frame = clock.to_lane();
        self
    }
}

/// Which way round a display's subpixels run.
///
/// It decides which channel of a per-channel coverage tile is which, and getting it backwards
/// puts a colour fringe on the wrong side of every stroke.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubpixelOrder {
    /// Red, green, blue — the overwhelmingly common arrangement.
    #[default]
    RedToBlue,
    /// Blue, green, red.
    BlueToRed,
}

#[cfg(test)]
mod tests {
    use super::{Globals, SubpixelOrder};
    use zgui_geom::Size;

    #[test]
    fn the_target_extent_is_never_zero() {
        let globals = Globals::new(Size::new(0, -4), SubpixelOrder::RedToBlue);
        assert_eq!(globals.viewport[0], 1.0);
        assert_eq!(globals.viewport[1], 1.0);
    }

    #[test]
    fn the_subpixel_order_reaches_the_shader_as_a_flag() {
        assert_eq!(
            Globals::new(Size::new(8, 8), SubpixelOrder::RedToBlue).text[2],
            0.0
        );
        assert_eq!(
            Globals::new(Size::new(8, 8), SubpixelOrder::BlueToRed).text[2],
            1.0
        );
    }

    #[test]
    fn a_half_resolution_target_halves_its_extent_and_says_so() {
        use crate::target::scale::TargetScale;

        let full = Globals::for_target(
            Size::new(64, 32),
            TargetScale::Full,
            SubpixelOrder::default(),
        );
        let half = Globals::for_target(
            Size::new(64, 32),
            TargetScale::Half,
            SubpixelOrder::default(),
        );
        assert_eq!(full.viewport, [64.0, 32.0, 1.0, 1.0]);
        assert_eq!(half.viewport, [32.0, 16.0, 0.5, 0.5]);
        // The two lanes are what the shader divides by to recover a device pixel, so a target
        // whose extent halved and whose scale did not would place every clip twice as far out.
        assert_eq!(
            half.viewport[0] * (1.0 / half.viewport[2]),
            full.viewport[0]
        );
    }

    #[test]
    fn the_correction_is_the_one_computed_for_the_display_gamma() {
        let globals = Globals::new(Size::new(8, 8), SubpixelOrder::default());
        assert_eq!(
            globals.gamma_ratios,
            zgui_color::gamma_correction_ratios(super::DISPLAY_GAMMA)
        );
    }
}
