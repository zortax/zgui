//! Assembling a frame's segments, before any pass is opened.

use zgui_geom::{Device, Point, Rect, Size};

use crate::bind::globals::{Globals, SubpixelOrder};
use crate::buffer::slots::SlotBuffer;
use crate::buffer::vectors::VectorInstances;
use crate::frame::segment::{EncoderOp, PassLoad, PlannedDraw, PlannedPass, Segment};
use crate::frame::target::TargetRef;
use crate::gpu::device::Gpu;
use crate::pipeline::blur::BlurParams;
use crate::pipeline::composite::CompositeParams;
use crate::pipeline::external::ExternalParams;
use crate::target::group_pool::{GroupPool, GroupSlot};
use crate::target::scale::TargetScale;

/// A frame, split into the passes and encoder operations that execute it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FramePlan {
    /// The pieces, in the order they run.
    pub segments: Vec<Segment>,
    /// Every planned draw; a pass names a contiguous range of these.
    pub draws: Vec<PlannedDraw>,
    /// How many composites named content this phase does not rasterise.
    pub deferred: u32,
    /// How many groups asked for a blend mode that is not plain source-over.
    pub unsupported_blends: u32,
    /// How many groups could not be isolated because the target pool was exhausted.
    pub unisolated: u32,
    /// How many instanced batches were left out of a pass whose scissor they miss.
    pub culled: u32,
}

impl FramePlan {
    /// Every pass of the plan.
    pub fn passes(&self) -> impl Iterator<Item = &PlannedPass> {
        self.segments.iter().filter_map(Segment::pass)
    }

    /// The draws of one pass.
    pub fn draws_of(&self, pass: &PlannedPass) -> &[PlannedDraw] {
        &self.draws[pass.draws.clone()]
    }

    /// The segments cut at each vector round: the round to rasterise first, if any, and the run of
    /// segments up to the next one.
    pub fn runs(&self) -> impl Iterator<Item = (Option<usize>, &[Segment])> {
        let rounds =
            core::iter::once(None).chain(self.segments.iter().filter_map(Segment::round).map(Some));
        rounds.zip(self.segments.split(|segment| segment.round().is_some()))
    }
}

/// What decides a per-target block: the kind of target, its origin and the region it covers.
type GlobalsKey = (usize, Point<i32, Device>, Size<i32, Device>);

/// Builds a [`FramePlan`], lending targets and staging blocks as it goes.
///
/// Everything the recorder will need is resolved here: which target each pass writes, what its
/// scissor is, which block each draw reads and where in the buffer that block sits. The recorder
/// then opens one pass per pass segment and drops it before the next encoder operation, which is
/// the whole reason for planning first — a live pass holds the encoder borrowed, and a recorder
/// that discovered that half way through would have to discard the borrow to carry on.
pub struct PlanBuilder<'gpu> {
    /// The device targets are allocated on.
    gpu: &'gpu Gpu,
    /// The pool isolated targets are lent by.
    pool: &'gpu mut GroupPool,
    /// Where per-target blocks are staged.
    globals: &'gpu mut SlotBuffer,
    /// Where per-draw blocks are staged.
    blocks: &'gpu mut SlotBuffer,
    /// Where the quads of every vector composite are staged.
    vectors: &'gpu mut VectorInstances,
    /// Where each mark item's block is staged.
    mark_draws: &'gpu mut SlotBuffer,
    /// The offset of each mark item's block this frame, by remap position.
    mark_blocks: std::collections::HashMap<usize, u32>,
    /// The texel extent of one bin page.
    mark_extent: Size<i32, Device>,
    /// Which way round the display's subpixels run.
    subpixel_order: SubpixelOrder,
    /// What this frame's application effects are told about it.
    frame_clock: zgui_scene::FrameClock,
    /// Where each interned parameter block was staged, by the slot the scene interned it under.
    effect_offsets: &'gpu [u32],
    /// The device-pixel region of the surface.
    region: Size<i32, Device>,
    /// What the composed target holds, which a copy out of it has to match.
    composed_format: wgpu::TextureFormat,
    /// The texel extent the composed target is allocated at.
    composed_extent: Size<i32, Device>,
    /// The first per-target block staged for each kind of target at the surface origin: full
    /// resolution, half resolution, and a bin page. Each holds the region it covers and its
    /// offset.
    staged_globals: [Option<(Size<i32, Device>, u32)>; 3],
    /// Every other staged block, by kind, origin and covered region.
    staged_other: Vec<(GlobalsKey, u32)>,
    /// The plan so far.
    plan: FramePlan,
    /// The pass currently being filled.
    open: Option<PlannedPass>,
    /// Every target lent while building, so none is leaked when the plan is finished.
    lent: Vec<GroupSlot>,
    /// Targets lent but not yet written into, which the first pass on each has to discard.
    unwritten: Vec<GroupSlot>,
    /// The vector round whose passes are in the scratch at this point of the plan.
    round: usize,
}

impl<'gpu> PlanBuilder<'gpu> {
    /// A builder over `region` device pixels.
    #[allow(
        clippy::too_many_arguments,
        reason = "a plan names every resource it draws from; grouping them would hide one"
    )]
    pub fn new(
        gpu: &'gpu Gpu,
        pool: &'gpu mut GroupPool,
        globals: &'gpu mut SlotBuffer,
        blocks: &'gpu mut SlotBuffer,
        vectors: &'gpu mut VectorInstances,
        mark_draws: &'gpu mut SlotBuffer,
        subpixel_order: SubpixelOrder,
        frame_clock: zgui_scene::FrameClock,
        effect_offsets: &'gpu [u32],
        region: Size<i32, Device>,
        composed_format: wgpu::TextureFormat,
        composed_extent: Size<i32, Device>,
    ) -> Self {
        Self {
            gpu,
            pool,
            globals,
            blocks,
            vectors,
            mark_draws,
            mark_blocks: std::collections::HashMap::new(),
            mark_extent: Size::new(0, 0),
            subpixel_order,
            frame_clock,
            effect_offsets,
            region,
            composed_format,
            composed_extent,
            staged_globals: [None; 3],
            staged_other: Vec::new(),
            plan: FramePlan::default(),
            open: None,
            lent: Vec::new(),
            unwritten: Vec::new(),
            round: 0,
        }
    }

    /// The device-pixel region of the surface.
    pub fn region(&self) -> Size<i32, Device> {
        self.region
    }

    /// The texel extent of whatever `target` is.
    ///
    /// This is the *allocation*, not the region drawn into: targets are rounded to a size class,
    /// and anything sampling one divides by what it holds rather than by what is used of it.
    pub fn extent_of(&self, target: TargetRef) -> Size<i32, Device> {
        match target {
            TargetRef::Composed => self.composed_extent,
            TargetRef::Pool(slot) => slot.allocated(),
            TargetRef::MarksPage(_) => self.mark_extent,
        }
    }

    /// Lends a target for isolated content at `scale` covering `region`, or reports that the pool
    /// could not.
    pub fn acquire(&mut self, scale: TargetScale, region: Rect<i32, Device>) -> Option<GroupSlot> {
        self.acquire_in(scale, GroupPool::FORMAT, region)
    }

    /// Lends a target for a group's content at `scale` covering `region`, or reports that the pool
    /// could not.
    ///
    /// A `plain` group can be held in the composed format, see [`GroupPool::format_for`].
    pub fn acquire_group(
        &mut self,
        scale: TargetScale,
        region: Rect<i32, Device>,
        plain: bool,
    ) -> Option<GroupSlot> {
        let format = self.pool.format_for(plain, self.composed_format);
        self.acquire_in(scale, format, region)
    }

    /// Lends a target covering `region` that holds the same thing `like` does, at its resolution,
    /// which is what a copy needs.
    ///
    /// Never reduces the resolution: a copy into a target of another resolution puts every texel
    /// at the wrong place. The copy measures each side from its own origin, so the two origins
    /// can differ. Returns `None` when the budget will not take the target.
    pub fn acquire_like(
        &mut self,
        like: TargetRef,
        region: Rect<i32, Device>,
    ) -> Option<GroupSlot> {
        let slot = self.pool.acquire_exact_covering(
            self.gpu,
            like.scale(),
            self.format_of(like),
            region,
        )?;
        Some(self.lend(slot))
    }

    /// What a target holds.
    pub fn format_of(&self, target: TargetRef) -> wgpu::TextureFormat {
        match target.slot() {
            Some(slot) => slot.format(),
            None => self.composed_format,
        }
    }

    /// Lends a target at `scale` in `format` covering `region`, or reports that the pool could
    /// not.
    fn acquire_in(
        &mut self,
        scale: TargetScale,
        format: wgpu::TextureFormat,
        region: Rect<i32, Device>,
    ) -> Option<GroupSlot> {
        let slot = self
            .pool
            .acquire_covering(self.gpu, scale, format, region)?;
        Some(self.lend(slot))
    }

    /// Records a lease the pool made.
    fn lend(&mut self, slot: GroupSlot) -> GroupSlot {
        self.lent.push(slot);
        // A lease starts with whatever the previous one left behind, so the first pass of this one
        // discards it. Tracking it per lease rather than per frame is what makes a reused target
        // safe: the alternative is a filter reading a stranger's content outside its own region.
        self.unwritten.push(slot);
        slot
    }

    /// Whether a lent target still holds whatever the lease before it left there.
    ///
    /// True until the first pass that writes into it, because that pass is the one that discards
    /// it — see `PlanBuilder::acquire_in`. Anything about to *read* a lent target has to ask: a
    /// lease nothing wrote into was never cleared, so reading it reads a stranger's pixels.
    pub fn is_unwritten(&self, slot: GroupSlot) -> bool {
        self.unwritten.contains(&slot)
    }

    /// Returns a lent target.
    pub fn release(&mut self, slot: GroupSlot) {
        self.pool.release(slot);
        if let Some(at) = self.lent.iter().position(|held| *held == slot) {
            self.lent.remove(at);
        }
        // The lease is over and it never discarded what it was given, so the obligation goes with
        // it: the next lease of this slot pushes its own, and one left standing here would let the
        // pass after that one skip the clear it owes.
        self.unwritten.retain(|held| *held != slot);
    }

    /// Stages one blur block and returns the offset naming it.
    pub fn stage_blur(&mut self, params: &BlurParams) -> u32 {
        self.blocks.stage(params)
    }

    /// Stages one application filter block and returns the offset naming it.
    pub fn stage_effect_filter(
        &mut self,
        params: &crate::pipeline::effect_filter::EffectFilterParams,
    ) -> u32 {
        self.blocks.stage(params)
    }

    /// What this frame's application effects are told about it.
    pub fn frame_clock(&self) -> zgui_scene::FrameClock {
        self.frame_clock
    }

    /// The dynamic offset naming the parameter block interned under `slot`.
    pub fn effect_offset(&self, slot: zgui_scene::ShaderParamsSlot) -> Option<u32> {
        self.effect_offsets.get(slot.0 as usize).copied()
    }

    /// Stages one composite block and returns the offset naming it.
    pub fn stage_composite(&mut self, params: &CompositeParams) -> u32 {
        self.blocks.stage(params)
    }

    /// Stages one external-texture block and returns the offset naming it.
    pub fn stage_external(&mut self, params: &ExternalParams) -> u32 {
        self.blocks.stage(params)
    }

    /// Sets the texel extent of one bin page, which the bin passes' block describes.
    pub fn set_mark_extent(&mut self, extent: Size<i32, Device>) {
        self.mark_extent = extent;
    }

    /// The offset of the block naming the mark item at `position`, staging `draw` the first time.
    ///
    /// One block per item and frame: a union item's coverage draws and its composite read the
    /// same block in every damage rectangle.
    pub fn stage_mark_draw(
        &mut self,
        position: usize,
        draw: impl FnOnce() -> crate::pipeline::marks::MarkDraw,
    ) -> u32 {
        if let Some(&offset) = self.mark_blocks.get(&position) {
            return offset;
        }
        let offset = self.mark_draws.stage(&draw());
        self.mark_blocks.insert(position, offset);
        offset
    }

    /// Makes `round` the vector round in the scratch before the next draw reads it.
    ///
    /// Round zero is rasterised before the frame is recorded. Any other round, and round zero again
    /// after a later one, cuts the plan: the open pass ends, the round is rasterised, and the same
    /// pass opens again and keeps what it drew.
    pub fn round(&mut self, round: usize) {
        if self.round == round {
            return;
        }
        self.round = round;
        let open = self.open.as_ref().map(|pass| (pass.target, pass.scissor));
        debug_assert!(
            !matches!(open, Some((TargetRef::MarksPage(_), _))),
            "a marks page opens again with a clear, so a round must not cut one"
        );
        self.end_pass();
        self.plan.segments.push(Segment::Round(round));
        if let Some((target, scissor)) = open {
            self.begin_pass(target, scissor);
        }
    }

    /// Stages the quads of one vector composite and returns the instance range naming them.
    pub fn stage_vector(
        &mut self,
        instances: impl IntoIterator<Item = crate::pipeline::vector::VectorInstance>,
    ) -> (u32, u32) {
        self.vectors.stage(instances)
    }

    /// The offset of the block describing `target`, staging it the first time.
    fn globals_for(&mut self, target: TargetRef) -> u32 {
        let (kind, region) = match target {
            TargetRef::MarksPage(_) => (2, self.mark_extent),
            TargetRef::Composed => (0, self.region),
            TargetRef::Pool(slot) => (
                usize::from(slot.scale() == TargetScale::Half),
                slot.covered(),
            ),
        };
        let origin = target.origin();
        if origin == Point::ORIGIN
            && let Some((held, offset)) = self.staged_globals[kind]
            && held == region
        {
            return offset;
        }
        let key = (kind, origin, region);
        if let Some(&(_, offset)) = self.staged_other.iter().find(|(held, _)| *held == key) {
            return offset;
        }
        let block = Globals::for_target(region, target.scale(), self.subpixel_order)
            .with_frame(self.frame_clock)
            .with_origin(origin);
        let offset = self.globals.stage(&block);
        if origin == Point::ORIGIN && self.staged_globals[kind].is_none() {
            self.staged_globals[kind] = Some((region, offset));
        } else {
            self.staged_other.push((key, offset));
        }
        offset
    }

    /// Opens a pass into `target`, scissored to `scissor`, ending whatever preceded it.
    pub fn begin_pass(&mut self, target: TargetRef, scissor: Rect<i32, Device>) {
        self.end_pass();
        let globals = self.globals_for(target);
        let load = match target.slot() {
            Some(slot) if self.unwritten.contains(&slot) => {
                self.unwritten.retain(|held| *held != slot);
                PassLoad::Discard
            }
            // A bin page holds only this frame's coverage.
            _ if matches!(target, TargetRef::MarksPage(_)) => PassLoad::Discard,
            _ => PassLoad::Keep,
        };
        let start = self.plan.draws.len();
        self.open = Some(PlannedPass {
            target,
            load,
            scissor,
            globals,
            draws: start..start,
        });
    }

    /// Adds a draw to the open pass.
    ///
    /// # Panics
    ///
    /// Panics if no pass is open, which would mean a draw with no attachment behind it.
    pub fn draw(&mut self, draw: PlannedDraw) {
        let open = self
            .open
            .as_mut()
            .expect("a draw is always planned into an open pass");
        self.plan.draws.push(draw);
        open.draws.end = self.plan.draws.len();
    }

    /// Closes the open pass, dropping it if it drew nothing or may write nothing.
    pub fn end_pass(&mut self) {
        let Some(pass) = self.open.take() else {
            return;
        };
        if pass.draws.is_empty() || pass.scissor.is_empty() {
            // A pass that is not recorded did not discard anything either, and the lease still
            // owes its first clear to whichever pass writes it next.
            if pass.load == PassLoad::Discard
                && let Some(slot) = pass.target.slot()
            {
                self.unwritten.push(slot);
            }
            return;
        }
        self.plan.segments.push(Segment::Pass(pass));
    }

    /// Records an operation that needs the encoder, ending whatever pass preceded it.
    pub fn encoder(&mut self, op: EncoderOp) {
        self.end_pass();
        self.plan.segments.push(Segment::Encoder(op));
    }

    /// Notes a draw this phase leaves out: a composite whose content it does not rasterise, or a
    /// batch the target cannot hold.
    pub fn defer(&mut self) {
        self.plan.deferred += 1;
    }

    /// Notes a group asking to composite by something other than plain source-over.
    pub fn note_unsupported_blend(&mut self) {
        self.plan.unsupported_blends += 1;
    }

    /// Notes a group the pool could not lend a target for.
    pub fn note_unisolated(&mut self) {
        self.plan.unisolated += 1;
    }

    /// Notes an instanced batch left out of the open pass, because it writes nothing inside the
    /// pass's scissor.
    pub fn note_culled(&mut self) {
        self.plan.culled += 1;
    }

    /// Finishes the plan, returning every lent target to the pool.
    pub fn finish(mut self) -> FramePlan {
        self.end_pass();
        for slot in core::mem::take(&mut self.lent) {
            self.pool.release(slot);
        }
        self.plan
    }
}
