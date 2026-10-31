//! Planning a `backdrop-filter`.

use zgui_geom::{Device, Rect};

use crate::frame::build::PlanBuilder;
use crate::frame::segment::{EncoderOp, PlannedDraw};
use crate::frame::target::TargetRef;

/// Copies `region` of `beneath` into `into`.
///
/// The capture is a copy rather than a read, because a fragment shader cannot read the attachment
/// it is writing. It is also the reason a backdrop is the one primitive whose correctness depends
/// on the damage set: what it filters is the composite so far, so every pixel it samples has to be
/// one *this* frame has already drawn. Sampling a pixel the frame did not redraw reads the
/// previous frame's composite — which already contains this filter's own output, so a frosted
/// panel smears a little further every frame until the whole panel is fog.
///
/// A **kept** copy is the exception, and the reason it is one: outside the damage it holds an
/// earlier frame's composite from *beneath* the panel, which at a pixel no frame since has redrawn
/// is the same picture rather than the fogged one.
pub fn capture(
    builder: &mut PlanBuilder<'_>,
    beneath: TargetRef,
    into: TargetRef,
    region: Rect<i32, Device>,
) {
    if region.is_empty() {
        return;
    }
    if let Some(under) = beneath.slot()
        && !under.holds(region)
    {
        // The target beneath holds only part of the region, so the copy writes only part of the
        // capture. The rest has to read transparent, as a texel of the target beneath outside its
        // own region does. The clear removes what the previous lease left there.
        builder.begin_pass(into, region);
        builder.draw(PlannedDraw::Clear);
        builder.end_pass();
    }
    builder.encoder(EncoderOp::Capture {
        source: beneath,
        destination: into,
        region,
    });
}

/// Lends a target to copy what lies beneath into, matching what it is copied from.
///
/// Returns `None` when the pool could not lend one, in which case the region is left as it is: an
/// unfiltered backdrop is the content it was meant to frost, which is a visible degradation and
/// not a wrong picture.
pub fn scratch(
    builder: &mut PlanBuilder<'_>,
    beneath: TargetRef,
    region: Rect<i32, Device>,
) -> Option<TargetRef> {
    Some(TargetRef::Pool(builder.acquire_like(beneath, region)?))
}

/// Whether the copy for a backdrop over `beneath` can be the kept one.
///
/// The kept copy is allocated like the composed target and holds what it holds, and a copy between
/// two textures requires them to agree — so a backdrop *inside* a group, whose composite so far is
/// that group's own half-float target, takes a scratch copy of that instead. The damage was grown
/// on the understanding that the kept copy would be used, so a frame that finds otherwise here
/// says so and the one after it redraws everything.
pub fn keepable(beneath: TargetRef) -> bool {
    beneath == TargetRef::Composed
}
