//! Planning the separable gaussian.

use zgui_geom::{Device, Rect};

use crate::frame::build::PlanBuilder;
use crate::frame::segment::PlannedDraw;
use crate::frame::target::TargetRef;
use crate::pipeline::blur::{BlurAxis, BlurParams};
use crate::target::scale::TargetScale;

/// What a blur left behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blurred {
    /// The target holding the result.
    pub target: TargetRef,
}

/// Plans a gaussian of `deviation` device pixels over `region`, reading `source`.
///
/// Three passes: a snapped 2:1 downsample, then one pass along each axis at that resolution. A
/// separable gaussian is two one-dimensional convolutions rather than one two-dimensional one, and
/// running the pair at half resolution costs a quarter as much again for a difference that a blur
/// is by definition insensitive to.
///
/// Every pass is scissored to the region, so a blurred panel costs its own area rather than the
/// window's, and the sampling lattice is anchored to the device origin, so the result does not
/// shift when the blurred content moves by a fraction of a pixel.
///
/// Returns `None` when the pool could not lend the two scratch targets, which is the one case
/// where there is nothing to do but composite the content unfiltered.
pub fn plan(
    builder: &mut PlanBuilder<'_>,
    source: TargetRef,
    region: Rect<i32, Device>,
    deviation: f32,
) -> Option<Blurred> {
    let ping = builder.acquire(TargetScale::Half)?;
    let pong = match builder.acquire(TargetScale::Half) {
        Some(pong) => pong,
        None => {
            builder.release(ping);
            return None;
        }
    };
    let ping = TargetRef::Pool(ping);
    let pong = TargetRef::Pool(pong);

    let source_extent = builder.extent_of(source);
    let half_extent = builder.extent(TargetScale::Half);

    let downsample = builder.stage_blur(&BlurParams::downsample(
        source_extent,
        source.scale(),
        half_extent,
        region,
    ));
    // Every region a pass carries is in device pixels; a pass into a half-resolution target has
    // its scissor converted where the pass is opened, so nothing here halves anything twice.
    builder.begin_pass(ping, region);
    builder.draw(PlannedDraw::Blur {
        source,
        params: downsample,
        downsample: true,
    });

    for (axis, from, to) in [
        (BlurAxis::Horizontal, ping, pong),
        (BlurAxis::Vertical, pong, ping),
    ] {
        let params = builder.stage_blur(&BlurParams::axis(
            half_extent,
            TargetScale::Half,
            axis,
            deviation,
            region,
        ));
        builder.begin_pass(to, region);
        builder.draw(PlannedDraw::Blur {
            source: from,
            params,
            downsample: false,
        });
    }

    // The second axis wrote back into `ping`, so `pong` is scratch again the moment the chain is
    // finished with it. Returning it here rather than at the end of the frame is what keeps a
    // chain of several blurs to two scratch targets rather than two per blur.
    if let Some(slot) = pong.slot() {
        builder.release(slot);
    }
    Some(Blurred { target: ping })
}
