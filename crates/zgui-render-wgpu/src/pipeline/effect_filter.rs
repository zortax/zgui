//! What one application filter pass is told.

use bytemuck::{Pod, Zeroable};
use zgui_geom::{Device, Point, Rect, Size};
use zgui_scene::FrameClock;

use crate::target::scale::TargetScale;

/// The block one filter pass reads.
///
/// It is the blur's block with the geometry an application effect needs in place of the kernel's:
/// where the element's own origin is, how big it is, and what moment it is. An effect is written
/// against its own box, so everything here exists to put a destination texel back into that box's
/// coordinates.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct EffectFilterParams {
    /// The source's extent in texels, then the destination's.
    pub extents: [f32; 4],
    /// The element's origin in device pixels, then the source's and the destination's texels per
    /// device pixel.
    pub placement: [f32; 4],
    /// The part of the source that was written, in source texels: the two corners.
    ///
    /// Reads are clamped to it. Everything outside is either a texel this pass's region did not
    /// cover or one a previous lease left behind, and an effect that read either would give a
    /// different answer depending on how much of the frame was being redrawn.
    pub valid: [f32; 4],
    /// The element's extent in device pixels, then the clock.
    pub element: [f32; 4],
    /// Device pixels per CSS pixel, then three unused lanes.
    pub frame: [f32; 4],
    /// The device pixel the source's first texel holds, then the same for the destination.
    pub origins: [f32; 4],
}

impl EffectFilterParams {
    /// The block for a pass reading `source` and writing the whole of `region`.
    ///
    /// `source` and `destination` are each target's extent in texels, its scale, and the device
    /// pixel its first texel holds.
    pub fn new(
        region: Rect<i32, Device>,
        source: (Size<i32, Device>, TargetScale, Point<i32, Device>),
        destination: (Size<i32, Device>, TargetScale, Point<i32, Device>),
        clock: FrameClock,
    ) -> Self {
        let (source_extent, source_scale, source_origin) = source;
        let (destination_extent, destination_scale, destination_origin) = destination;
        let read = region.translate(Point::ORIGIN.offset_from(source_origin));
        let valid = [
            read.origin.x as f32 * source_scale.factor(),
            read.origin.y as f32 * source_scale.factor(),
            (read.origin.x + read.size.width) as f32 * source_scale.factor(),
            (read.origin.y + read.size.height) as f32 * source_scale.factor(),
        ];
        Self {
            extents: [
                source_extent.width.max(1) as f32,
                source_extent.height.max(1) as f32,
                destination_extent.width.max(1) as f32,
                destination_extent.height.max(1) as f32,
            ],
            placement: [
                region.origin.x as f32,
                region.origin.y as f32,
                source_scale.factor(),
                destination_scale.factor(),
            ],
            valid,
            element: [
                region.size.width as f32,
                region.size.height as f32,
                clock.seconds,
                clock.delta,
            ],
            frame: [clock.scale, 0.0, 0.0, 0.0],
            origins: [
                source_origin.x as f32,
                source_origin.y as f32,
                destination_origin.x as f32,
                destination_origin.y as f32,
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EffectFilterParams;
    use crate::target::scale::TargetScale;
    use zgui_geom::{Point, Rect, Size};
    use zgui_scene::FrameClock;

    fn params() -> EffectFilterParams {
        EffectFilterParams::new(
            Rect::new(Point::new(12, 8), Size::new(40, 20)),
            (Size::new(256, 256), TargetScale::Full, Point::ORIGIN),
            (Size::new(256, 256), TargetScale::Full, Point::ORIGIN),
            FrameClock {
                seconds: 1.5,
                delta: 0.016,
                scale: 2.0,
            },
        )
    }

    #[test]
    fn the_element_the_effect_is_written_against_is_the_region_being_filtered() {
        let params = params();
        assert_eq!(params.placement[0], 12.0);
        assert_eq!(params.placement[1], 8.0);
        assert_eq!(params.element[0], 40.0);
        assert_eq!(params.element[1], 20.0);
    }

    #[test]
    fn reads_are_clamped_to_the_part_of_the_source_the_pass_covered() {
        let params = params();
        assert_eq!(params.valid, [12.0, 8.0, 52.0, 28.0]);
    }

    #[test]
    fn the_clock_and_the_scale_reach_the_effect() {
        let params = params();
        assert_eq!(params.element[2], 1.5);
        assert_eq!(params.element[3], 0.016);
        assert_eq!(params.frame[0], 2.0);
    }

    /// A half-resolution source is read in its own texels, so the mapping has to say so.
    #[test]
    fn a_half_resolution_source_reports_its_own_texels() {
        let params = EffectFilterParams::new(
            Rect::new(Point::new(0, 0), Size::new(40, 20)),
            (Size::new(128, 128), TargetScale::Half, Point::ORIGIN),
            (Size::new(256, 256), TargetScale::Full, Point::ORIGIN),
            FrameClock::default(),
        );
        assert_eq!(params.placement[2], 0.5);
        assert_eq!(params.placement[3], 1.0);
        assert_eq!(params.valid, [0.0, 0.0, 20.0, 10.0]);
    }

    #[test]
    fn reads_are_measured_from_the_source_origin_and_the_element_stays_on_the_device() {
        let params = EffectFilterParams::new(
            Rect::new(Point::new(12, 8), Size::new(40, 20)),
            (Size::new(64, 64), TargetScale::Full, Point::new(10, 4)),
            (Size::new(64, 64), TargetScale::Full, Point::new(8, 0)),
            FrameClock::default(),
        );
        assert_eq!(params.valid, [2.0, 4.0, 42.0, 24.0]);
        assert_eq!(params.placement[..2], [12.0, 8.0]);
        assert_eq!(params.origins, [10.0, 4.0, 8.0, 0.0]);
    }
}
