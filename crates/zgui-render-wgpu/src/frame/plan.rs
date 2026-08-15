//! Splitting a frame's batches at the points an encoder operation has to run.

use zgui_bits::DamageSet;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_render::ExternalTexture;
use zgui_scene::{BackdropFilter, Batch, GroupBoundary, Scene};

use crate::filter::chain::Chain;
use crate::filter::{self, Filtered};
use crate::frame::build::{FramePlan, PlanBuilder};
use crate::frame::damage;
use crate::frame::segment::PlannedDraw;
use crate::frame::target::TargetRef;
use crate::frame::vector;
use crate::pipeline::composite::CompositeParams;
use crate::pipeline::external::ExternalParams;
use crate::target::scale::TargetScale;

/// A group whose content is being drawn into a target of its own.
struct OpenGroup {
    /// The target it writes.
    target: TargetRef,
    /// Where it composites back to.
    parent: TargetRef,
    /// Its marker.
    boundary: GroupBoundary,
    /// The region of the composed target its content may touch.
    region: Rect<i32, Device>,
}

/// Splits `scene` into the passes and encoder operations that draw it, once per damage rectangle.
///
/// Everything is planned before a single pass is opened. A live render pass holds the command
/// encoder borrowed, so a group beginning, a backdrop capture or a copy between targets cannot run
/// while one is alive — and the split points are exactly those operations. Planning first makes
/// them a value that can be read and asserted; discovering them while recording would mean
/// discarding the borrow that states the constraint.
///
/// One rectangle is one full replay of the batch stream under a different scissor, rather than a
/// re-culled stream. Which primitives survive a damage set is decided where the display list is
/// built, from the fragments' own ink; here the rectangles are only where the writes land.
pub fn plan_segments(
    mut builder: PlanBuilder<'_>,
    scene: &Scene,
    damage: &DamageSet,
    used: Rect<i32, Device>,
    externals: &dyn Fn(zgui_scene::ExternalTextureId) -> Option<ExternalTexture>,
    vectors: Option<&zgui_render::VectorPlan>,
) -> FramePlan {
    // A backdrop reads the target beneath it, so the rectangles have to cover what it reads.
    let backdrops: Vec<_> = scene
        .primitives
        .backdrops
        .iter()
        .filter(|backdrop| !backdrop.reads_only_what_it_writes())
        .map(|backdrop| rounded_out(backdrop.source))
        .collect();

    for rect in damage::rects_covering_backdrops(damage, used, &backdrops) {
        plan_rect(&mut builder, scene, rect, used, externals, vectors);
    }
    builder.finish()
}

/// Plans one damage rectangle's worth of the batch stream.
fn plan_rect(
    builder: &mut PlanBuilder<'_>,
    scene: &Scene,
    rect: Rect<i32, Device>,
    used: Rect<i32, Device>,
    externals: &dyn Fn(zgui_scene::ExternalTextureId) -> Option<ExternalTexture>,
    vectors: Option<&zgui_render::VectorPlan>,
) {
    let mut stack: Vec<OpenGroup> = Vec::new();
    let mut current = TargetRef::Composed;
    let mut scissor = rect;

    builder.begin_pass(current, scissor);
    // The rectangle is about to be redrawn, so what is in it goes first. It is a draw because a
    // render pass clears its whole attachment or none of it, and clearing all of it would throw
    // away every pixel this frame is relying on not having to redraw.
    builder.draw(PlannedDraw::Clear);

    for batch in scene.batches() {
        match batch {
            Batch::Group(index) => {
                let Some(boundary) = scene.primitives.groups.get(index) else {
                    continue;
                };
                if boundary.is_start {
                    match begin_group(builder, boundary, current, used) {
                        Some(group) => {
                            scissor = group.region;
                            current = group.target;
                            stack.push(group);
                            builder.begin_pass(current, scissor);
                        }
                        None => {
                            // The pool had nothing to lend. The content is drawn straight into the
                            // target beneath, which is the same picture whenever the group's own
                            // effect is the identity and a visibly flatter one when it is not.
                            builder.note_unisolated();
                        }
                    }
                } else if let Some(group) = stack.pop() {
                    // The composite lands in whatever is beneath the group, and what may be
                    // written there is that target's own region — the damage rectangle only when
                    // it *is* the composed target. Cutting a composite into an enclosing group's
                    // target down to the damage rectangle would leave the enclosing group's own
                    // filter reading texels the inner group never wrote, and a filtered pixel would
                    // then differ between a partial redraw and a whole one.
                    scissor = stack.last().map_or(rect, |outer| outer.region);
                    end_group(builder, &group, scissor);
                    current = group.parent;
                    builder.begin_pass(current, scissor);
                }
            }
            Batch::Backdrop(index) => {
                let Some(backdrop) = scene.primitives.backdrops.get(index) else {
                    continue;
                };
                plan_backdrop(builder, backdrop, current, scissor, used);
                builder.begin_pass(current, scissor);
            }
            Batch::External(index) => {
                let Some(quad) = scene.primitives.externals.get(index) else {
                    continue;
                };
                match externals(quad.texture) {
                    Some(texture) => {
                        let params = builder.stage_external(&ExternalParams::of(quad, &texture));
                        builder.draw(PlannedDraw::External {
                            texture: quad.texture,
                            params,
                        });
                    }
                    // A quad naming a texture nothing registered cannot be drawn against another
                    // one: that would show a stranger's pixels rather than nothing.
                    None => builder.defer(),
                }
            }
            // Vector content is rasterised outside this crate and composited back in at exactly
            // this point in the order, which is the whole of what makes the interleave correct:
            // submission order is z-order, so an ordinary draw inserted here is exactly right.
            Batch::Vector(index) => {
                match vectors.and_then(|plan| plan.passes.get(index).map(|pass| (plan, pass))) {
                    Some((plan, pass)) => {
                        let (first, count) = builder.stage_vector(vector::instances_of(plan, pass));
                        if count == 0 {
                            builder.defer();
                        } else {
                            builder.draw(PlannedDraw::Vector {
                                target: pass.target,
                                first,
                                count,
                            });
                        }
                    }
                    // No rasteriser is attached, or it resourced fewer passes than were planned. Either
                    // way the composite names content that does not exist, and it is counted rather
                    // than drawn against whatever the scratch happens to hold.
                    None => builder.defer(),
                }
            }
            other => builder.draw(PlannedDraw::Batch(other)),
        }
    }

    // A group whose end marker never arrived would leave a target open. Markers are matched pairs
    // and nothing may drop one, so this is a display list that was already wrong; closing them
    // here is what keeps a wrong display list from becoming a lost target.
    while let Some(group) = stack.pop() {
        let enclosing = stack.last().map_or(rect, |outer| outer.region);
        end_group(builder, &group, enclosing);
        builder.begin_pass(group.parent, enclosing);
    }
    builder.end_pass();
}

/// Lends a group its target and works out the region its content may touch.
fn begin_group(
    builder: &mut PlanBuilder<'_>,
    boundary: &GroupBoundary,
    parent: TargetRef,
    used: Rect<i32, Device>,
) -> Option<OpenGroup> {
    // The content is drawn over everything the group *reads*, not only over what it writes. A blur
    // samples outside the element's box, and a target populated only where the composite lands
    // would fade to an edge that is an artefact of the damage rectangle rather than of the filter.
    let region = enclosing(boundary.source, used)?;
    let target = TargetRef::Pool(builder.acquire(TargetScale::Full)?);
    Some(OpenGroup {
        target,
        parent,
        boundary: boundary.clone(),
        region,
    })
}

/// Filters a finished group and composites it back into the target beneath.
///
/// `scissor` is the region of the target beneath that this frame may write: the damage rectangle
/// when that target is the composed one, and the enclosing group's own region when it is not.
fn end_group(builder: &mut PlanBuilder<'_>, group: &OpenGroup, scissor: Rect<i32, Device>) {
    // Closed here rather than by the first pass the composite opens, so that what follows can ask
    // whether anything was drawn into the group at all: a pass with no draws is not recorded, and
    // recording it is what would have cleared the target.
    builder.end_pass();
    if let Some(slot) = group.target.slot()
        && builder.is_unwritten(slot)
    {
        // Nothing was drawn into this lease, so it never discarded what the lease before it left
        // there — and compositing it now would put a stranger's content on the screen, inside this
        // group's region, where it stays until something else happens to draw over it. An empty
        // element carrying `opacity` is enough to open one: a label with no text still has a box,
        // and a column of them down the side of a list is a column of windows onto whatever the
        // pool last held.
        //
        // Skipped rather than cleared first, because that is what an empty group *is*: no pixels,
        // and a filter over no pixels is no pixels whatever the filter says.
        builder.release(slot);
        return;
    }
    let chain = Chain::of(&group.boundary.filters);
    let filtered = filter::plan(builder, &chain, group.target, group.region);
    if !is_source_over(&group.boundary) {
        builder.note_unsupported_blend();
    }

    // A content filter composites over everything it read, so a blur fades out past the element's
    // box exactly as CSS says it does, and the shape of the fade comes from the group's own alpha
    // against the transparent surround its target was cleared to. The clip chain is what gives it
    // a defined edge where the content asked for one, and it is the same chain, evaluated by the
    // same function, that every other pipeline applies.
    composite(
        builder,
        &filtered,
        group.parent,
        scissor,
        group.region,
        group.boundary.clip.0,
        group.boundary.opacity,
    );
    for slot in filtered
        .shadows
        .iter()
        .filter_map(|shadow| shadow.source.slot())
        .chain(filtered.target.slot())
        .chain(group.target.slot())
    {
        builder.release(slot);
    }
}

/// Captures what lies beneath a backdrop, filters it, and draws the result.
fn plan_backdrop(
    builder: &mut PlanBuilder<'_>,
    backdrop: &BackdropFilter,
    beneath: TargetRef,
    scissor: Rect<i32, Device>,
    used: Rect<i32, Device>,
) {
    // The batch stream is replayed once per damage rectangle, so a rectangle nowhere near this
    // backdrop still reaches here. It writes nothing inside such a rectangle, and capturing what
    // it would have read is both wasted and impossible — the read lies outside what this pass is
    // allowed to write.
    if clamped(backdrop.bounds, scissor, used).is_empty() {
        return;
    }
    let Some(source) = enclosing(backdrop.source, used) else {
        return;
    };
    // What a backdrop reads has to be inside what this frame has already written into the target
    // beneath it, or it samples the previous frame's composite — which already contains this
    // filter's own output, and the panel smears a little further every frame. A filter with no
    // reach is exempt and safe: it reads only the pixel it writes, which the scissor already
    // covers.
    debug_assert!(
        backdrop.reads_only_what_it_writes() || scissor.contains_rect(source),
        "a backdrop reads {source:?}, which the region {scissor:?} being written does not contain"
    );
    let chain = Chain::of(&backdrop.filters);
    let Some(filtered) = crate::filter::backdrop::plan(builder, beneath, &chain, source) else {
        builder.note_unisolated();
        return;
    };
    // A backdrop composites only over what it writes, which is what gives a frosted panel the
    // defined shape its clip describes rather than a soft fade past its own edge.
    let bounds = clamped(backdrop.bounds, scissor, used);
    composite(
        builder,
        &filtered,
        beneath,
        scissor,
        bounds,
        backdrop.clip.0,
        1.0,
    );
    for slot in filtered
        .shadows
        .iter()
        .filter_map(|shadow| shadow.source.slot())
        .chain(filtered.target.slot())
    {
        builder.release(slot);
    }
}

/// Draws a filtered result, and any shadow behind it, into `destination`.
#[allow(
    clippy::too_many_arguments,
    reason = "a composite names every part of what it draws; grouping them would hide one"
)]
fn composite(
    builder: &mut PlanBuilder<'_>,
    filtered: &Filtered,
    destination: TargetRef,
    scissor: Rect<i32, Device>,
    bounds: Rect<i32, Device>,
    clip: u32,
    opacity: f32,
) {
    builder.begin_pass(destination, scissor);
    for shadow in &filtered.shadows {
        let mut params = shadow.params;
        params.bounds = rect_bounds(bounds);
        params.control[0] = clip as f32;
        params = params.with_opacity(opacity);
        let staged = builder.stage_composite(&params);
        builder.draw(PlannedDraw::Composite {
            source: shadow.source,
            params: staged,
        });
    }
    let params = CompositeParams::new(
        bounds.to_unit(),
        builder.extent_of(filtered.target),
        filtered.target.scale(),
        clip,
    )
    .with_opacity(opacity)
    .with_matrix(filtered.matrix);
    let staged = builder.stage_composite(&params);
    builder.draw(PlannedDraw::Composite {
        source: filtered.target,
        params: staged,
    });
}

/// Whether a group composites by plain source-over.
fn is_source_over(boundary: &GroupBoundary) -> bool {
    use zgui_scene::peniko::{BlendMode, Compose, Mix};

    boundary.blend == BlendMode::new(Mix::Normal, Compose::SrcOver)
}

/// The region a composite that reads `source` needs written, or `None` if it is off the surface.
///
/// It is what the composite reads, cut to the surface, and deliberately **not** widened to the
/// rectangle being redrawn. A region that depended on the damage set would make a filter's own
/// answer depend on how much of the frame was being redrawn, and a filtered pixel would then
/// differ between a partial redraw and a whole one — which is the one property everything about
/// redrawing part of a frame rests on.
fn enclosing(source: Rect<DevicePx, Device>, used: Rect<i32, Device>) -> Option<Rect<i32, Device>> {
    rounded_out(source).intersection(used)
}

/// A device-pixel rectangle cut to what is being redrawn and to the surface.
fn clamped(
    bounds: Rect<DevicePx, Device>,
    rect: Rect<i32, Device>,
    used: Rect<i32, Device>,
) -> Rect<i32, Device> {
    rounded_out(bounds)
        .intersection(rect)
        .and_then(|clipped| clipped.intersection(used))
        .unwrap_or(Rect::ZERO)
}

/// A fractional rectangle grown to whole device pixels.
fn rounded_out(rect: Rect<DevicePx, Device>) -> Rect<i32, Device> {
    Rect::from_corners(
        zgui_geom::Point::new(rect.left().0.floor() as i32, rect.top().0.floor() as i32),
        zgui_geom::Point::new(rect.right().0.ceil() as i32, rect.bottom().0.ceil() as i32),
    )
}

/// A whole-pixel rectangle as a composite block holds it.
fn rect_bounds(rect: Rect<i32, Device>) -> [f32; 4] {
    [
        rect.origin.x as f32,
        rect.origin.y as f32,
        rect.size.width as f32,
        rect.size.height as f32,
    ]
}
