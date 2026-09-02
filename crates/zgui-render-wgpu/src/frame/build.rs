//! Assembling a frame's segments, before any pass is opened.

use zgui_geom::{Device, Rect, Size};

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
}

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
    /// Which way round the display's subpixels run.
    subpixel_order: SubpixelOrder,
    /// What this frame's application effects are told about it.
    frame_clock: zgui_scene::FrameClock,
    /// Where each interned parameter block was staged, by the slot the scene interned it under.
    effect_offsets: &'gpu [u32],
    /// The region every target covers.
    region: Size<i32, Device>,
    /// What the composed target holds, which a copy out of it has to match.
    composed_format: wgpu::TextureFormat,
    /// The texel extent the composed target is allocated at.
    composed_extent: Size<i32, Device>,
    /// The offsets of the per-target blocks already staged, by resolution.
    staged_globals: [Option<u32>; 2],
    /// The plan so far.
    plan: FramePlan,
    /// The pass currently being filled.
    open: Option<PlannedPass>,
    /// Every target lent while building, so none is leaked when the plan is finished.
    lent: Vec<GroupSlot>,
    /// Targets lent but not yet written into, which the first pass on each has to discard.
    unwritten: Vec<GroupSlot>,
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
            subpixel_order,
            frame_clock,
            effect_offsets,
            region,
            composed_format,
            composed_extent,
            staged_globals: [None; 2],
            plan: FramePlan::default(),
            open: None,
            lent: Vec::new(),
            unwritten: Vec::new(),
        }
    }

    /// The region every target covers.
    pub fn region(&self) -> Size<i32, Device> {
        self.region
    }

    /// The texel extent a target at `scale` is allocated at.
    ///
    /// This is the *allocation*, not the region drawn into: targets are rounded to a size class,
    /// and anything sampling one divides by what it holds rather than by what is used of it.
    pub fn extent(&self, scale: TargetScale) -> Size<i32, Device> {
        self.pool.allocated_extent(scale)
    }

    /// The texel extent of whatever `target` is.
    pub fn extent_of(&self, target: TargetRef) -> Size<i32, Device> {
        match target {
            TargetRef::Composed => self.composed_extent,
            TargetRef::Pool(slot) => self.pool.allocated_extent(slot.scale()),
        }
    }

    /// Lends a target for isolated content at `scale`, or reports that the pool could not.
    pub fn acquire(&mut self, scale: TargetScale) -> Option<GroupSlot> {
        self.acquire_in(scale, GroupPool::FORMAT)
    }

    /// Lends a target holding the same thing `like` does, which is what a copy needs.
    pub fn acquire_like(&mut self, like: TargetRef) -> Option<GroupSlot> {
        self.acquire_in(TargetScale::Full, self.format_of(like))
    }

    /// What a target holds.
    pub fn format_of(&self, target: TargetRef) -> wgpu::TextureFormat {
        match target.slot() {
            Some(slot) => slot.format(),
            None => self.composed_format,
        }
    }

    /// Lends a target at `scale` in `format`, or reports that the pool could not.
    fn acquire_in(&mut self, scale: TargetScale, format: wgpu::TextureFormat) -> Option<GroupSlot> {
        let slot = self.pool.acquire(self.gpu, scale, format)?;
        self.lent.push(slot);
        // A lease starts with whatever the previous one left behind, so the first pass of this one
        // discards it. Tracking it per lease rather than per frame is what makes a reused target
        // safe: the alternative is a filter reading a stranger's content outside its own region.
        self.unwritten.push(slot);
        Some(slot)
    }

    /// Whether a lent target still holds whatever the lease before it left there.
    ///
    /// True until the first pass that writes into it, because that pass is the one that discards
    /// it — see [`PlanBuilder::acquire_in`]. Anything about to *read* a lent target has to ask: a
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

    /// Stages the quads of one vector composite and returns the instance range naming them.
    pub fn stage_vector(
        &mut self,
        instances: impl IntoIterator<Item = crate::pipeline::vector::VectorInstance>,
    ) -> (u32, u32) {
        self.vectors.stage(instances)
    }

    /// The offset of the block describing a target at `scale`, staging it the first time.
    fn globals_for(&mut self, scale: TargetScale) -> u32 {
        let index = usize::from(scale == TargetScale::Half);
        match self.staged_globals[index] {
            Some(offset) => offset,
            None => {
                let block = Globals::for_target(self.region, scale, self.subpixel_order)
                    .with_frame(self.frame_clock);
                let offset = self.globals.stage(&block);
                self.staged_globals[index] = Some(offset);
                offset
            }
        }
    }

    /// Opens a pass into `target`, scissored to `scissor`, ending whatever preceded it.
    pub fn begin_pass(&mut self, target: TargetRef, scissor: Rect<i32, Device>) {
        self.end_pass();
        let globals = self.globals_for(target.scale());
        let load = match target.slot() {
            Some(slot) if self.unwritten.contains(&slot) => {
                self.unwritten.retain(|held| *held != slot);
                PassLoad::Discard
            }
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

    /// Closes the open pass, dropping it if it drew nothing.
    pub fn end_pass(&mut self) {
        let Some(pass) = self.open.take() else {
            return;
        };
        if pass.draws.is_empty() {
            // A pass that drew nothing is not recorded, so it did not discard anything either and
            // the lease still owes its first clear to whichever pass writes it next.
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

    /// Notes a composite whose content this phase does not rasterise.
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

    /// Finishes the plan, returning every lent target to the pool.
    pub fn finish(mut self) -> FramePlan {
        self.end_pass();
        for slot in core::mem::take(&mut self.lent) {
            self.pool.release(slot);
        }
        self.plan
    }
}
