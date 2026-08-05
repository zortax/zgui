//! What happens to a display list: the frame, and the two steps either side of it.

use zgui_bits::DamageSet;
use zgui_profile::{Counter, Phase, counter};
use zgui_render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, SkipReason, TextureHandle,
};
use zgui_scene::Scene;

use crate::frame::build::PlanBuilder;
use crate::frame::damage;
use crate::frame::pass::Recorder;
use crate::frame::plan::plan_segments;
use crate::frame::present;
use crate::pipeline::kind::PipelineKind;
use crate::renderer::WgpuRenderer;
use crate::target::acquire::{Acquisition, SurfaceAction};

impl Renderer for WgpuRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        self.gpu.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        // Every trigger for this — a reconfiguration, a resize, a change of scale factor — changes
        // what the surface is, so the frame after it cannot rely on what the composed target holds
        // relative to it. A change of scale alone moves no allocation and would otherwise pass
        // through here unnoticed.
        self.full_damage_next |= self.target != target;
        self.target = target;
        self.resize(target.size);
    }

    fn target(&self) -> Option<RenderTarget> {
        Some(self.target)
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        self.atlas()
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let _frame = Phase::Render.span().entered();
        zgui_profile::latency::mark("draw.in");

        // A surface whose configuration was thrown away is configured again here rather than at the
        // point below where a *resize* is applied, and the placement is the whole of it: everything
        // past this line records against the surface, so the frame that finds one unconfigured is
        // the frame that returns before ever reaching the rebuild. A surface invalidated by a lost
        // acquisition would then be unconfigured for the rest of the window's life — every frame
        // skipping at this line, nothing ever calling the one thing that would fix it, and a window
        // that is running, answering input and never drawing again.
        //
        // Nothing is stalled by doing it early: the wait a rebuild costs is a wait for work still in
        // flight, and a surface that has never been configured, or has stopped being, has none.
        if !self.presentation.is_configured() {
            self.presentation.apply_pending(&self.gpu);
        }
        // The one early exit that keeps its damage, because it records nothing.
        if !self.presentation.is_configured() {
            return FrameOutcome::Skipped(SkipReason::Unconfigured);
        }
        // A lost device takes everything with it, so the frame that notices rebuilds before it
        // records anything, and reports that it did: the next frame redraws the whole surface
        // because nothing on the new device holds what the old one drew.
        if self.gpu.loss().is_lost() {
            match self.recover() {
                Ok(()) => return FrameOutcome::Recovered,
                Err(failure) => {
                    tracing::error!(%failure, "the device was lost and could not be rebuilt");
                    // Named apart from a rejected acquisition, and the difference is whether asking
                    // again can help. A validation failure is one moment and a run of them is what
                    // escalates to this; this is the end of the escalation, and a frame that asked
                    // for the next one would rebuild the same unbuildable device for ever.
                    return FrameOutcome::Skipped(SkipReason::DeviceUnavailable);
                }
            }
        }
        if self.presentation.reconfigure_pending() {
            self.resize(self.target.size);
        }
        // Anything that threw the composed target away, or changed the surface underneath it,
        // leaves it holding nothing this frame may rely on — so the damage set it was handed is
        // widened to all of it exactly once, here, rather than at each of the places that noticed.
        let everything = DamageSet::full();
        let damage = if core::mem::take(&mut self.full_damage_next) {
            &everything
        } else {
            damage
        };

        // A frame that damages nothing has nothing to compose and nothing to present: the surface
        // already holds these pixels, because the composed target it was last blitted from has not
        // changed. Presenting it anyway is not merely wasted work — under a queued presentation
        // mode it spends a swap-chain image, and the next frame that *does* change something then
        // waits a whole refresh interval to acquire one. This is checked after the widening above,
        // so a frame that has to redraw everything for a reason of the renderer's own never
        // reaches it.
        if damage.is_empty() && !self.present_composed_next {
            zgui_profile::latency::mark("draw.undamaged");
            return FrameOutcome::Skipped(SkipReason::Undamaged);
        }

        let formats = self.presentation.formats();
        zgui_profile::latency::mark("r.tables");
        self.buffers.prepare_tables(scene);
        self.buffers.begin_frame(&self.gpu);

        // Everything the rasteriser does happens before this frame's encoder exists, because an
        // implementation submits command buffers of its own. Ordering stays exact anyway: nothing
        // it submits writes the target this frame composes into.
        zgui_profile::latency::mark("r.vectors");
        let vectors = self.rasterise_vectors(scene, damage.is_full());

        // Everything is planned before a pass is opened, because a live pass holds the encoder
        // borrowed and the points a frame has to be cut at are exactly the operations that need
        // it. The plan is also where the frame's blocks are staged and its isolated targets are
        // lent, so that recording is nothing but issuing what was decided.
        zgui_profile::latency::mark("r.plan");
        let externals = |id| self.externals.get(&id).map(|attached| attached.texture);
        let plan = {
            let builder = PlanBuilder::new(
                &self.gpu,
                &mut self.groups,
                &mut self.buffers.globals,
                &mut self.buffers.blocks,
                &mut self.buffers.vectors,
                self.subpixel_order,
                self.composed.used().size,
                self.composed.format(),
                self.composed.allocated(),
            );
            plan_segments(
                builder,
                scene,
                damage,
                self.composed.used(),
                &externals,
                vectors.as_ref(),
            )
        };
        zgui_profile::latency::mark("r.encoder");
        let mut encoder =
            self.gpu
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("zgui.frame"),
                });
        zgui_profile::latency::mark("r.buffers");
        let uploaded = self.buffers.upload_frame(&self.gpu, &mut encoder, scene);
        let upload_allocations = self.buffers.upload_allocations();
        self.buffers.finish_uploads();
        if upload_allocations == 0 {
            zgui_profile::latency::mark("r.record");
        } else {
            zgui_profile::latency::note(
                "r.record",
                format!("{upload_allocations} upload chunks allocated"),
            );
        }
        let recorded = Recorder {
            gpu: &self.gpu,
            pipelines: &mut self.pipelines,
            buffers: &self.buffers,
            atlas: &self.atlas,
            pool: &self.groups,
            composed: &self.composed,
            sampler: &self.sampler,
            externals: &self.externals,
            vectors: self.vectors.as_deref(),
        }
        .record(&mut encoder, &plan);
        self.groups.release_all();
        if plan.deferred > 0 {
            tracing::debug!(
                composites = plan.deferred,
                "composites naming content this renderer does not rasterise"
            );
        }
        if plan.unsupported_blends > 0 {
            tracing::warn!(
                groups = plan.unsupported_blends,
                "groups asking to composite by something other than source-over"
            );
        }

        // The swap chain is rebuilt here rather than where the resize was noticed. The rebuild
        // waits for the device to go idle, and everything above has just given it a frame's worth
        // of CPU time to become so; at the top of the frame the same call stalls on a submission
        // that was made moments earlier.
        self.presentation.apply_pending(&self.gpu);

        // Acquisition happens after every draw is recorded and immediately before the copy, so a
        // failed acquisition costs only the copy — and the frame's work is submitted either way,
        // which is why its damage is retired either way.
        zgui_profile::latency::mark("acq.in");
        let asked = std::time::Instant::now();
        let presented = present::acquire(&self.presentation, &mut self.faults);
        // Measured around the call and nowhere else. It is the frame's slack seen from the inside:
        // how much earlier than it needed to be the frame was started.
        self.acquire_block = asked.elapsed();
        zgui_profile::latency::note("acq.out", presented.acquisition.name());
        if crate::gpu::surface::human_visible_wait(self.acquire_block) {
            tracing::warn!(
                stage = "acquire",
                elapsed_ms = self.acquire_block.as_millis() as u64,
                width = self.target.size.width,
                height = self.target.size.height,
                present_mode = ?crate::gpu::surface::PRESENT_MODE,
                frame_latency = crate::gpu::surface::FRAME_LATENCY,
                acquisition = presented.acquisition.name(),
                "surface operation blocked the event-loop thread"
            );
        }
        let mut draw_calls = recorded.draw_calls;
        if let Some(view) = &presented.view {
            self.blit(&mut encoder, view, formats.blit_undoes_srgb());
            draw_calls += 1;
        }
        self.gpu.queue().submit([encoder.finish()]);
        self.buffers.recall_uploads();
        zgui_profile::latency::mark("sub.out");

        let response = presented.acquisition.response();
        // Composition succeeded whichever answer acquisition gave, but only a presenting answer
        // copied that persistent target to the surface. A retry with no new damage must still run
        // through acquisition and the final blit rather than taking the undamaged early return.
        self.present_composed_next = !response.presents;
        if response.presents
            && let Some(notify) = &self.pre_present
        {
            notify();
        }
        if let Some(texture) = presented.surface_texture {
            texture.present();
        }
        zgui_profile::latency::mark("pres.out");

        let redrawn: u64 = damage::rects(damage, self.composed.used())
            .iter()
            .map(|rect| damage::area(*rect))
            .sum();
        counter::add(Counter::DamagePx, redrawn);
        let stats = FrameStats {
            draw_calls,
            vector_passes: scene.pass_plan().passes.len() as u32,
            damage_px: redrawn,
            bytes_uploaded: uploaded,
            memory: self.memory(),
        };

        if response.force_full_damage {
            // The surface changed underneath the frame and nothing observed what the compositor
            // did with it, so what the composed target holds is no longer known to be on screen.
            self.full_damage_next = true;
        }
        match response.surface_action() {
            SurfaceAction::Nothing => {}
            SurfaceAction::Reconfigure => self.presentation.request_reconfigure(),
            SurfaceAction::Recreate => self.presentation.invalidate(),
        }
        if presented.acquisition == Acquisition::Validation {
            // A run of rejected acquisitions long enough to escalate is a device that will not
            // start presenting again on its own, so it is recorded as lost and the next frame
            // takes the rebuild path. Counting to the limit and then only logging would be a
            // threshold with nothing behind it, and a program retrying an unusable device for
            // ever.
            if self.gpu.loss().note_validation_failure() {
                self.gpu.loss().report(
                    wgpu::DeviceLostReason::Unknown,
                    "acquisition failed validation repeatedly",
                );
            }
        } else {
            self.gpu.loss().note_acquisition_succeeded();
        }
        presented.acquisition.outcome(stats)
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        let handle = TextureHandle(self.next_handle);
        self.next_handle += 1;
        // A description with no resource behind it cannot be drawn, and nothing here can invent
        // one: the texture belongs to whoever produced the frames. `attach_external` is where it
        // arrives, and until it does the quad is counted as undrawn rather than drawn wrongly.
        let described = ExternalTexture { handle, ..texture };
        if let Some(attached) = self.externals.get_mut(&texture.id) {
            attached.texture = described;
        }
        self.pending_externals.insert(texture.id, described);
        handle
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.externals
            .retain(|_, attached| attached.texture.handle != handle);
        self.pending_externals
            .retain(|_, texture| texture.handle != handle);
    }

    fn target_pool(&self) -> zgui_render::TargetPoolReport {
        // The composed target is deliberately absent. It is the surface's own size, the next frame
        // needs it, and freeing it would buy the length of one reallocation; the group pool is the
        // part that grows with what the document nests and stays grown after it stops.
        zgui_render::TargetPoolReport {
            resident: self.groups.bytes(),
            lent: self.groups.lent_bytes(),
            leases: self.groups.leases(),
        }
    }

    fn release_cached_targets(&mut self) -> u64 {
        self.groups.release_unused()
    }

    fn acquire_block(&self) -> std::time::Duration {
        self.acquire_block
    }

    fn memory(&self) -> MemoryReport {
        // The rasteriser's own footprint is added component by component rather than as one total,
        // because its fixed cost and its scratch scale with completely different things — the first
        // with nothing at all, the second with the surface and with how much is rasterised at once —
        // and one number would hide whichever of the two is spending the headroom.
        let own = MemoryReport {
            fixed: 0,
            targets: self.composed.bytes() + self.groups.bytes(),
            scratch: 0,
            atlases: self.atlas.bytes(),
            buffers: self.buffers.bytes() + self.atlas.staging_bytes(),
        };
        match &self.vectors {
            Some(raster) => own.plus(raster.memory()),
            None => own,
        }
    }
}

impl WgpuRenderer {
    /// Runs this frame's vector work, before the frame's own encoder exists.
    ///
    /// A frame with no surviving vector item runs no rasterisation at all — not an empty pass, which
    /// is very far from free — so the whole of this is behind one question asked of the display
    /// list's own plan.
    fn rasterise_vectors(
        &mut self,
        scene: &Scene,
        full_repaint: bool,
    ) -> Option<zgui_render::VectorPlan> {
        let planned = scene.pass_plan();
        // The pass count and the clip-layer count are **not** recorded here. Both are properties of
        // the display list — which passes there are and what absorbing their residuals costs are
        // decided before any device sees them — and the display list records them when it is
        // finished. Recording them again here would double every figure a budget reads, and would
        // make them numbers only a real device could produce.
        //
        // The warning is a different matter: it is about a frame, it needs to know whether the
        // frame is a full repaint, and only here is that known.
        if let Some(warning) = planned.warning(full_repaint) {
            tracing::debug!("{}", warning.message());
        }

        let fatal = self.vector_shortfall_is_fatal;
        let raster = self.vectors.as_deref_mut()?;
        let mut plan = raster.plan(planned);
        if plan.is_empty() {
            return None;
        }
        // Mandatory, and before anything reads: a rasterisation that fails while reporting success
        // would otherwise leave the previous frame's content in a reused scratch, which composites
        // as wrong pixels rather than as missing ones and has nothing to notice it by.
        raster.clear_targets(&plan);
        let placements = zgui_scene::Placements::of(&scene.spatial);
        let mut frame = zgui_render::VectorFrame::new(
            &plan,
            &scene.primitives.vectors,
            &scene.clips,
            &scene.paints,
            &placements,
        );
        debug_assert_eq!(
            plan.passes.len(),
            planned.passes.len(),
            "a resourced plan names one pass per planned pass, in the same order: a composite is \
             named by its index, so a plan that dropped one would draw every later composite from \
             the wrong pass"
        );
        // The frame borrows the plan, and the plan is what the shortfall arm below shortens, so
        // the result is taken out of the borrow before it is read.
        let outcome = raster.prepare(&mut frame);
        match outcome {
            Ok(()) => Some(plan),
            // More passes than the scratch can keep apart. The passes that were finished are each
            // in a scratch of their own and composite correctly; only the rest have nowhere to be
            // read from, and a plan naming fewer passes than were planned leaves exactly those
            // composites counted as undrawn. Losing the tail is a fraction of the frame's vector
            // content, where returning nothing here loses all of it.
            Err(zgui_render::VectorError::OutOfCapacity { detail, prepared }) => {
                counter::bump(Counter::VectorFramesDropped);
                assert!(
                    !fatal,
                    "the vector rasteriser ran out of capacity and {} of {} passes were drawn: \
                     {detail}",
                    prepared,
                    plan.passes.len()
                );
                tracing::warn!(
                    prepared,
                    planned = plan.passes.len(),
                    %detail,
                    "the vector rasteriser ran out of capacity, so this frame draws the passes \
                     that fit"
                );
                plan.passes.truncate(prepared);
                (!plan.is_empty()).then_some(plan)
            }
            Err(failure) => {
                tracing::warn!(%failure, "the vector rasteriser could not prepare this frame");
                // The scratch was cleared and nothing was written into it, so compositing it now
                // would draw nothing at all. Reporting no plan leaves the composites counted as
                // undrawn instead, which is the same picture and a number somebody can see.
                counter::bump(Counter::VectorFramesDropped);
                None
            }
        }
    }

    /// Copies the composed target onto whatever is being presented to.
    fn blit(&mut self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, undo: bool) {
        let kind = if undo {
            PipelineKind::BlitUndoSrgb
        } else {
            PipelineKind::Blit
        };
        let format = self.presentation.formats().present_attachment();
        let Some(pipeline) = self.pipelines.get(&self.gpu, kind, format) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("zgui.present"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Every pixel is written, so nothing has to be preserved and no clear is
                    // inserted ahead of the copy.
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.composed_binding, &[]);
        pass.draw(0..4, 0..1);
    }
}
