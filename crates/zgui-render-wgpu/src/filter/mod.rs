//! Running a `filter` or `backdrop-filter` chain over an isolated target.

pub mod backdrop;
pub mod blur;
pub mod chain;
pub mod drop_shadow;
pub mod matrix;

use zgui_geom::{Device, Rect};

use crate::filter::chain::{Chain, Step};
use crate::filter::matrix::ColorMatrix;
use crate::frame::build::PlanBuilder;
use crate::frame::segment::PlannedDraw;
use crate::frame::target::TargetRef;
use crate::pipeline::composite::CompositeParams;
use crate::target::scale::TargetScale;

/// One blurred, tinted copy to draw behind the filtered content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowLayer {
    /// The target the copy's coverage is read from.
    pub source: TargetRef,
    /// The block describing it.
    pub params: CompositeParams,
}

/// What a filter chain left for the composite to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Filtered {
    /// The target holding the filtered content.
    pub target: TargetRef,
    /// The map the composite applies as it samples, which is free.
    pub matrix: ColorMatrix,
    /// Copies to draw behind the content, in order.
    pub shadows: Vec<ShadowLayer>,
}

/// Plans `chain` over `source`, restricted to `region`.
///
/// The steps run in the order they were written, because they do not commute: an affine colour map
/// with a constant term applied before a blur and applied after it are two different pictures.
/// Only a run of per-pixel functions at the *end* of a chain costs nothing, and that is because
/// the composite that was going to draw the content anyway carries it.
///
/// When the pool cannot lend a target for a step, that step is skipped and counted rather than
/// faked: content that is one filter less blurred is a visible degradation, and content composited
/// into the wrong place is not a degradation at all.
pub fn plan(
    builder: &mut PlanBuilder<'_>,
    chain: &Chain,
    source: TargetRef,
    region: Rect<i32, Device>,
) -> Filtered {
    let (steps, folded) = chain.split();
    let mut filtered = Filtered {
        target: source,
        matrix: folded,
        shadows: Vec::new(),
    };
    for step in steps {
        match *step {
            Step::Matrix(matrix) => {
                if let Some(next) = materialise(builder, filtered.target, region, matrix) {
                    replace(builder, &mut filtered, source, next);
                }
            }
            Step::Blur(deviation) => {
                if let Some(blurred) = blur::plan(builder, filtered.target, region, deviation) {
                    replace(builder, &mut filtered, source, blurred.target);
                }
            }
            Step::DropShadow {
                offset_x,
                offset_y,
                blur,
                color,
            } => {
                if let Some(shadow) = drop_shadow::plan(
                    builder,
                    filtered.target,
                    region,
                    (offset_x, offset_y),
                    blur,
                    color,
                ) {
                    filtered.shadows.push(shadow);
                }
            }
        }
    }
    filtered
}

/// Points `filtered` at `next`, returning whatever scratch it held before.
///
/// The chain's own input belongs to the caller — it is the group's target, and the composite is
/// not the only thing that may still read it — so it is never returned here.
fn replace(
    builder: &mut PlanBuilder<'_>,
    filtered: &mut Filtered,
    input: TargetRef,
    next: TargetRef,
) {
    let previous = filtered.target;
    filtered.target = next;
    if previous != input
        && previous != next
        && let Some(slot) = previous.slot()
    {
        builder.release(slot);
    }
}

/// Writes `source` through `matrix` into a target of its own, for a map that is not the last step.
///
/// A map at the end of a chain never reaches here: the composite applies it while it samples, and
/// a pass for it would be a target written and read back for nothing.
fn materialise(
    builder: &mut PlanBuilder<'_>,
    source: TargetRef,
    region: Rect<i32, Device>,
    matrix: ColorMatrix,
) -> Option<TargetRef> {
    let slot = builder.acquire(TargetScale::Full)?;
    let destination = TargetRef::Pool(slot);
    let params = CompositeParams::new(
        region.to_unit(),
        builder.extent_of(source),
        source.scale(),
        zgui_scene::ClipId::ROOT.0,
    )
    .with_matrix(matrix);
    let params = builder.stage_composite(&params);
    builder.begin_pass(destination, region);
    builder.draw(PlannedDraw::Composite { source, params });
    Some(destination)
}
