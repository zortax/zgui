//! Planning the separable gaussian.

use zgui_geom::{Device, Edges, Rect};

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

/// How far one axis of a gaussian of `deviation` device pixels reads, in whole device pixels.
///
/// The same three deviations [`read_extent`](zgui_scene::read_extent) inflates a filtered region
/// by, rounded out, so the region a pass is given here and the region the damage was grown to
/// cannot disagree about the tail.
pub fn reach(deviation: f32) -> i32 {
    (zgui_scene::Filter::BLUR_EXTENT * deviation.max(0.0)).ceil() as i32
}

/// Plans a gaussian of `deviation` device pixels writing `output`, reading `source` where it holds
/// `valid`.
///
/// Three passes: a snapped 2:1 downsample, then one pass along each axis at that resolution. A
/// separable gaussian is two one-dimensional convolutions rather than one two-dimensional one, and
/// running the pair at half resolution costs a quarter as much again for a difference that a blur
/// is by definition insensitive to.
///
/// # Each pass writes only what the pass after it reads
///
/// The three do not cover the same rectangle. A pass along an axis reads its source [`reach`]
/// pixels away *along that axis alone*, so the one before it has to have written that much further
/// out — and only along that axis. Working back from `output`: the vertical pass writes it, the
/// horizontal pass writes it grown vertically, and the downsample writes it grown both ways. Every
/// one of those is cut to `valid`, because a read that leaves what the source holds is clamped to
/// its edge rather than left to whatever a previous lease put there.
///
/// That staging is the whole reason a blurred panel over a moving spinner costs the spinner's area
/// rather than the panel's. Giving `output` and `valid` the same rectangle collapses all three back
/// onto it, which is what a filter over its own isolated target wants: it reads only what it wrote.
///
/// The sampling lattice is anchored to the device origin throughout, so the result does not shift
/// when the blurred content moves by a fraction of a pixel.
///
/// Returns `None` when the pool could not lend the two scratch targets, which is the one case
/// where there is nothing to do but composite the content unfiltered.
pub fn plan(
    builder: &mut PlanBuilder<'_>,
    source: TargetRef,
    output: Rect<i32, Device>,
    valid: Rect<i32, Device>,
    deviation: f32,
) -> Option<Blurred> {
    debug_assert!(
        valid.contains_rect(output),
        "a blur writing {output:?} reads outside the {valid:?} its source holds"
    );
    let reach = reach(deviation);
    let downsampled = grown(output, reach, reach, valid);
    let horizontal = grown(output, 0, reach, valid);
    // Both scratch targets are leased over what the source holds. Every region a pass writes is
    // cut to it, so one lease covers all three, and its origin is a device pixel the surface has —
    // a lease taken over a slack-grown rectangle would hold texels for pixels outside the surface
    // and shift every origin the passes are given.
    let ping = builder.acquire(TargetScale::Half, valid)?;
    let pong = match builder.acquire(TargetScale::Half, valid) {
        Some(pong) => pong,
        None => {
            builder.release(ping);
            return None;
        }
    };
    let ping = TargetRef::Pool(ping);
    let pong = TargetRef::Pool(pong);

    // What each pass promises the next one it has written. A texel wider than the region it was
    // staged over, and never wider than what the source itself holds — see [`SLACK`].
    let promised = |region| grown(region, MARGIN, MARGIN, valid);

    let downsample = builder.stage_blur(&BlurParams::downsample(
        (builder.extent_of(source), source.origin()),
        source.scale(),
        (builder.extent_of(ping), ping.origin()),
        valid,
    ));
    // Every region a pass carries is in device pixels; a pass into a half-resolution target has
    // its scissor converted where the pass is opened, so nothing here halves anything twice.
    builder.begin_pass(ping, with_slack(downsampled, valid));
    builder.draw(PlannedDraw::Blur {
        source,
        params: downsample,
        downsample: true,
    });

    // Each axis is told what the pass before it wrote, which is what its reads are clamped to.
    for (axis, from, to, region, written) in [
        (
            BlurAxis::Horizontal,
            ping,
            pong,
            horizontal,
            promised(downsampled),
        ),
        (BlurAxis::Vertical, pong, ping, output, promised(horizontal)),
    ] {
        let params = builder.stage_blur(&BlurParams::axis(
            [builder.extent_of(from), builder.extent_of(to)],
            TargetScale::Half,
            [from.origin(), to.origin()],
            axis,
            deviation,
            written,
        ));
        builder.begin_pass(to, with_slack(region, valid));
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

/// `rect` grown by `x` pixels along x and `y` along y, cut to `within`.
fn grown(rect: Rect<i32, Device>, x: i32, y: i32, within: Rect<i32, Device>) -> Rect<i32, Device> {
    rect.outset(Edges::axes(x, y))
        .intersection(within)
        .unwrap_or(rect)
}

/// How far past the region it was staged over each pass promises to have written, in device pixels.
///
/// One half-resolution texel. The furthest tap of an axis pass lands exactly on the edge of what
/// the pass before it was staged over, and a read is clamped half a texel inside what it is told is
/// valid — so a promise of exactly that region would pull the outermost tap of every staged blur
/// half a texel inwards, which the frame drawn whole does not do. Cut to what the source itself
/// holds, so a chain staged over the whole of it promises exactly the whole of it and nothing
/// changes for the frames that were never staged.
const MARGIN: i32 = 2;

/// How far past what it promises each pass actually writes, in device pixels.
///
/// Two more half-resolution texels, because the reader lands outside the promise twice over: a
/// device-pixel scissor converted to half-resolution texels drops the odd last half pixel, and a
/// bilinear tap at the edge of a region weighs the texel beyond it. A pass that wrote exactly what
/// it promised would leave the reader taking the *unfiltered* downsample still sitting in the
/// target from the pass before — a sharp seam one pixel wide around every staged region.
///
/// The extra texels are as correct as the rest: each is the same shader over the same clamped
/// input, evaluated at its own position.
const SLACK: i32 = 4;

/// `rect` grown by everything a pass writes beyond the region it was staged over, cut to `within`.
///
/// The cut is what keeps a pass inside the target it writes: a lease is taken over what the source
/// holds, so a region grown past that names texels the lease does not have.
fn with_slack(rect: Rect<i32, Device>, within: Rect<i32, Device>) -> Rect<i32, Device> {
    grown(rect, MARGIN + SLACK, MARGIN + SLACK, within)
}
