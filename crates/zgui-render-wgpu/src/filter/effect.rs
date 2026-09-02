//! Planning one pass of an application's own shader over a group's content.

use zgui_geom::{Device, Rect};
use zgui_scene::{ShaderId, ShaderParamsSlot};

use crate::frame::build::PlanBuilder;
use crate::frame::segment::PlannedDraw;
use crate::frame::target::TargetRef;
use crate::pipeline::effect_filter::EffectFilterParams;
use crate::target::scale::TargetScale;

/// Plans one filter effect over `region`, reading `source`.
///
/// One pass at full resolution, scissored to the region, exactly as every other filtering step is.
/// It is full resolution rather than half because nothing here knows what the effect does: a blur
/// is by definition insensitive to the sample rate and an arbitrary function is not.
///
/// Returns `None` when the pool could not lend a target, which is the one case where there is
/// nothing to do but composite the content unfiltered.
pub fn plan(
    builder: &mut PlanBuilder<'_>,
    source: TargetRef,
    region: Rect<i32, Device>,
    shader: ShaderId,
    block: u32,
) -> Option<TargetRef> {
    let slot = builder.acquire(TargetScale::Full)?;
    let destination = TargetRef::Pool(slot);
    let params = builder.stage_effect_filter(&EffectFilterParams::new(
        region,
        builder.extent_of(source),
        source.scale(),
        builder.extent_of(destination),
        destination.scale(),
        builder.frame_clock(),
    ));
    builder.begin_pass(destination, region);
    builder.draw(PlannedDraw::Effect {
        source,
        shader,
        params,
        block,
    });
    Some(destination)
}

/// The offset of the effect's own parameter block, or `None` when the frame staged none for it.
pub fn block_of(builder: &PlanBuilder<'_>, params: ShaderParamsSlot) -> Option<u32> {
    builder.effect_offset(params)
}
