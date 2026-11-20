//! Running a `filter` or `backdrop-filter` chain over an isolated target.

pub mod backdrop;
pub mod blur;
pub mod chain;
pub mod drop_shadow;
pub mod effect;
pub mod matrix;

use zgui_geom::{Device, Edges, Rect};

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

/// Plans `chain` writing `output`, reading `source` where it holds `valid`.
///
/// The steps run in the order they were written, because they do not commute: an affine colour map
/// with a constant term applied before a blur and applied after it are two different pictures.
/// Only a run of per-pixel functions at the *end* of a chain costs nothing, and that is because
/// the composite that was going to draw the content anyway carries it.
///
/// Each step writes `output` grown by everything the steps after it read outside themselves, cut
/// to `valid`, so the last one writes exactly `output` and no pass covers a pixel nothing goes on
/// to read. Passing the same rectangle for both collapses the staging back onto it, which is what
/// a `filter` over its own isolated target wants: it reads only what it just wrote, so there is
/// nothing further out to grow towards.
///
/// When the pool cannot lend a target for a step, that step is skipped and counted rather than
/// faked: content that is one filter less blurred is a visible degradation, and content composited
/// into the wrong place is not a degradation at all.
pub fn plan(
    builder: &mut PlanBuilder<'_>,
    chain: &Chain,
    source: TargetRef,
    output: Rect<i32, Device>,
    valid: Rect<i32, Device>,
) -> Filtered {
    let (steps, folded) = chain.split();
    let regions = staging(steps, output, valid);
    let mut filtered = Filtered {
        target: source,
        matrix: folded,
        shadows: Vec::new(),
    };
    // What the target the next step reads is written over, which a skipped step leaves alone.
    let mut holds = valid;
    for (step, writes) in steps.iter().zip(regions) {
        match *step {
            Step::Matrix(matrix) => {
                if let Some(next) = materialise(builder, filtered.target, writes, matrix) {
                    replace(builder, &mut filtered, source, next);
                    holds = writes;
                }
            }
            Step::Blur(deviation) => {
                if let Some(blurred) =
                    blur::plan(builder, filtered.target, writes, holds, deviation)
                {
                    replace(builder, &mut filtered, source, blurred.target);
                    holds = writes;
                }
            }
            Step::Custom { shader, params, .. } => {
                // An effect whose parameters this frame staged nothing for is one the display list
                // named and the frame did not intern, which cannot happen for a scene that was
                // finished — and drawing it against whatever block happens to be bound would be a
                // rectangle full of a stranger's numbers.
                if let Some(block) = effect::block_of(builder, params)
                    && let Some(next) =
                        effect::plan(builder, filtered.target, writes, shader, block)
                {
                    replace(builder, &mut filtered, source, next);
                    holds = writes;
                }
            }
            Step::DropShadow {
                offset_x,
                offset_y,
                blur,
                color,
            } => {
                // A shadow is a layer drawn *behind* the content rather than a replacement for it,
                // so it neither advances the chain's target nor what that target holds.
                if let Some(shadow) = drop_shadow::plan(
                    builder,
                    filtered.target,
                    writes,
                    holds,
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

/// What each step of `steps` has to write for the one after it to be able to read.
///
/// Worked backwards from `output`, growing by each step's own reach as it goes and cutting every
/// answer to `valid`. It is deliberately the *sum* of the reaches beyond a step rather than a
/// per-step chain of them: a drop shadow reads its input without replacing it, so a later step's
/// reach and its own both fall on whatever wrote that input, and adding them is the answer that is
/// right for either arrangement.
fn staging(
    steps: &[Step],
    output: Rect<i32, Device>,
    valid: Rect<i32, Device>,
) -> Vec<Rect<i32, Device>> {
    let mut regions = Vec::with_capacity(steps.len());
    let mut beyond = 0;
    for step in steps.iter().rev() {
        regions.push(
            output
                .outset(Edges::uniform(beyond))
                .intersection(valid)
                .unwrap_or(output),
        );
        beyond += reach_of(step);
    }
    regions.reverse();
    regions
}

/// How far a step reads outside what it writes, in whole device pixels.
fn reach_of(step: &Step) -> i32 {
    match step {
        Step::Matrix(_) => 0,
        // A custom effect is scissored to its region and reads the source at the pixel it writes,
        // so like a per-pixel map it reaches nothing outside itself.
        Step::Custom { .. } => 0,
        Step::Blur(deviation) => blur::reach(*deviation),
        // The copy is displaced as well as blurred, so it reaches its own offset further on the
        // side it falls towards. One number for every side, because a rectangle grown by four
        // different amounts would still have to hold the largest of them somewhere.
        Step::DropShadow {
            offset_x,
            offset_y,
            blur,
            ..
        } => blur::reach(*blur) + offset_x.abs().max(offset_y.abs()).ceil() as i32,
    }
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
    let slot = builder.acquire(TargetScale::Full, region)?;
    let destination = TargetRef::Pool(slot);
    let params = CompositeParams::new(
        region.to_unit(),
        builder.extent_of(source),
        source.scale(),
        source.origin(),
        zgui_scene::ClipId::ROOT.0,
    )
    .with_matrix(matrix);
    let params = builder.stage_composite(&params);
    builder.begin_pass(destination, region);
    builder.draw(PlannedDraw::Composite { source, params });
    Some(destination)
}
