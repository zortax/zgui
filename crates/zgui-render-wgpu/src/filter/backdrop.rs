//! Planning a `backdrop-filter`.

use zgui_geom::{Device, Rect};

use crate::filter::chain::Chain;
use crate::filter::{self, Filtered};
use crate::frame::build::PlanBuilder;
use crate::frame::segment::EncoderOp;
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
/// Returns `None` when the pool could not lend a target for the copy, in which case the region is
/// left as it is: an unfiltered backdrop is the content it was meant to frost, which is a visible
/// degradation and not a wrong picture.
pub fn plan(
    builder: &mut PlanBuilder<'_>,
    beneath: TargetRef,
    chain: &Chain,
    region: Rect<i32, Device>,
) -> Option<Filtered> {
    let captured = TargetRef::Pool(builder.acquire_like(beneath)?);
    builder.encoder(EncoderOp::Capture {
        source: beneath,
        destination: captured,
        region,
    });
    Some(filter::plan(builder, chain, captured, region))
}
