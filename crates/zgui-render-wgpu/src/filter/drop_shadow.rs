//! Planning a `drop-shadow()`.

use zgui_geom::{Device, Rect};

use crate::filter::ShadowLayer;
use crate::filter::blur;
use crate::frame::build::PlanBuilder;
use crate::frame::target::TargetRef;
use crate::pipeline::composite::CompositeParams;

/// Plans a blurred, displaced copy of `source` in `color`.
///
/// `drop-shadow()` is not `box-shadow`: the shape it casts is the content's own alpha rather than
/// its box, so an icon with a hole casts a shadow with a hole. That is why it goes through the
/// blur chain over the isolated target instead of through the analytic rounded-rectangle shadow —
/// there is no rectangle to be analytic about.
///
/// Returns `None` when the pool could not lend the blur its scratch, in which case the content is
/// drawn without its shadow rather than with a wrong one.
pub fn plan(
    builder: &mut PlanBuilder<'_>,
    source: TargetRef,
    region: Rect<i32, Device>,
    offset: (f32, f32),
    deviation: f32,
    color: [f32; 4],
) -> Option<ShadowLayer> {
    let blurred = blur::plan(builder, source, region, deviation)?;
    let params = CompositeParams::new(
        region.to_unit(),
        builder.extent_of(blurred.target),
        blurred.target.scale(),
        zgui_scene::ClipId::ROOT.0,
    )
    .as_shadow(color, offset);
    Some(ShadowLayer {
        source: blurred.target,
        params,
    })
}
