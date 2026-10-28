//! Planning a `backdrop-filter`.

use zgui_geom::{Device, Rect};

use crate::filter::chain::Chain;
use crate::filter::{self, Filtered};
use crate::frame::build::PlanBuilder;
use crate::frame::segment::{EncoderOp, PlannedDraw};
use crate::frame::target::TargetRef;

/// Captures what lies beneath `region` in `beneath` and plans `chain` over the copy.
///
/// The capture is a copy rather than a read, because a fragment shader cannot read the attachment
/// it is writing. It is also the reason a backdrop is the one primitive whose correctness depends
/// on the damage set: what it filters is the composite so far, so every pixel it samples has to be
/// one *this* frame has already drawn. Sampling a pixel the frame did not redraw reads the
/// previous frame's composite — which already contains this filter's own output, so a frosted
/// panel smears a little further every frame until the whole panel is fog.
///
/// Returns the filtered result and the copy. The chain never releases its own input, so the caller
/// releases the copy after the composite unless the result still names it.
///
/// Returns `None` when the pool could not lend a target for the copy, in which case the region is
/// left as it is: an unfiltered backdrop is the content it was meant to frost, which is a visible
/// degradation and not a wrong picture.
pub fn plan(
    builder: &mut PlanBuilder<'_>,
    beneath: TargetRef,
    chain: &Chain,
    region: Rect<i32, Device>,
) -> Option<(Filtered, TargetRef)> {
    let slot = builder.acquire_like(beneath, region)?;
    let captured = TargetRef::Pool(slot);
    if let Some(under) = beneath.slot()
        && !under.holds(region)
    {
        // The target beneath holds only part of the region, so the copy writes only part of the
        // capture. The rest has to read transparent, as a texel of the target beneath outside its
        // own region does. The clear removes what the previous lease left there.
        builder.begin_pass(captured, region);
        builder.draw(PlannedDraw::Clear);
        builder.end_pass();
    }
    builder.encoder(EncoderOp::Capture {
        source: beneath,
        destination: captured,
        region,
    });
    Some((filter::plan(builder, chain, captured, region), captured))
}
