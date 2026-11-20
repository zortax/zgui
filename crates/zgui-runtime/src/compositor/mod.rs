//! Drawing a window through a compositor and its renderer.
//!
//! A [`CompositorRenderer`] stands in the window's renderer seam. The paint stage records the
//! frame's changes into a transaction, and drawing commits the transaction, builds the output's
//! frame, draws it and reports the outcome. The display list the window still assembles is not
//! drawn.

mod convert;
mod sink;

use compositor_api::{Compositor, OutputConfig};
use compositor_types::{
    Built, ClientId, Frame, FrameClock, FrameRequest, OutputId, SubpixelOrder, Transaction,
};
use web_time::Instant;
use zgui_bits::DamageSet;
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    SkipReason, TextureHandle,
};
use zgui_scene::Scene;

use crate::compositor::sink::Discard;

/// A window's renderer that draws through a compositor and the compositor's renderer.
pub struct CompositorRenderer {
    compositor: Box<dyn Compositor>,
    renderer: Box<dyn compositor_api::Renderer>,
    client: ClientId,
    output: OutputId,
    frame: Frame,
    target: Option<RenderTarget>,
    subpixel: Option<SubpixelOrder>,
    rejected: bool,
    started: Instant,
    sink: Discard,
}

impl CompositorRenderer {
    /// Creates a renderer that draws `compositor`'s frames with `renderer`, as one client and
    /// one output of `compositor`.
    pub fn new(
        mut compositor: Box<dyn Compositor>,
        renderer: Box<dyn compositor_api::Renderer>,
    ) -> Self {
        let (client, _ids) = compositor.register_client();
        let output = compositor.create_output(OutputConfig {
            size: [1, 1],
            scale: 1.0,
            opaque: true,
        });
        Self {
            compositor,
            renderer,
            client,
            output,
            frame: Frame::default(),
            target: None,
            subpixel: None,
            rejected: false,
            started: Instant::now(),
            sink: Discard,
        }
    }

    /// Returns the renderer the compositor's frames are drawn with.
    pub fn renderer(&self) -> &dyn compositor_api::Renderer {
        &*self.renderer
    }

    /// Returns the output the window draws into.
    pub fn output(&self) -> OutputId {
        self.output
    }

    /// Returns an empty transaction of the window's client.
    pub fn transaction(&mut self) -> Transaction {
        self.compositor.transaction(self.client)
    }

    /// Commits `transaction`.
    ///
    /// When the compositor rejects it, the window's client is registered again with nothing, and
    /// [`take_rejected`](Self::take_rejected) reports it.
    pub fn submit(&mut self, transaction: Transaction) {
        if let Err(error) = self.compositor.commit(transaction) {
            tracing::error!(target: "zgui::compositor", ?error, "a transaction was rejected");
            self.restart_client();
        }
    }

    /// Reports whether the compositor rejected a transaction since the last call.
    ///
    /// After a rejection the window's client starts again with nothing, so the caller records
    /// everything again.
    pub fn take_rejected(&mut self) -> bool {
        std::mem::take(&mut self.rejected)
    }

    /// Builds and draws the output's next frame.
    fn compose(&mut self) -> FrameOutcome {
        let caps = self.renderer.capabilities();
        let now_ns = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let request = FrameRequest {
            caps: &caps,
            age: self.renderer.target_age(),
            clock: FrameClock {
                now_ns,
                present_ns: now_ns,
                interval_ns: 0,
            },
            subpixel: self.subpixel,
        };
        if let Built::Idle { .. } =
            self.compositor
                .build_frame(self.output, &request, &mut self.frame)
        {
            return FrameOutcome::Skipped(SkipReason::Undamaged);
        }
        let outcome = self.renderer.draw(&self.frame);
        self.compositor
            .frame_done(self.output, self.frame.id, &outcome);
        convert::outcome(outcome)
    }

    /// Drops everything the window's client holds and registers it again.
    fn restart_client(&mut self) {
        self.compositor.unregister_client(self.client);
        let (client, _ids) = self.compositor.register_client();
        self.client = client;
        self.rejected = true;
    }
}

impl Renderer for CompositorRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        convert::capabilities(self.renderer.capabilities())
    }

    fn configure(&mut self, target: RenderTarget) {
        let converted = convert::target(target);
        self.renderer.configure(converted);
        self.compositor.configure_output(
            self.output,
            OutputConfig {
                size: converted.size,
                scale: converted.scale,
                opaque: converted.opaque,
            },
        );
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, _scene: &Scene, _damage: &DamageSet) -> FrameOutcome {
        self.compose()
    }

    fn set_subpixel_order(&mut self, order: zgui_render::SubpixelOrder) {
        let order = convert::subpixel(order);
        self.subpixel = Some(order);
        self.renderer.set_subpixel_order(order);
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        // External textures reach the compositor as texture slots, which this renderer does not
        // bind yet.
        tracing::warn!(target: "zgui::compositor", id = ?texture.id, "external texture dropped");
        texture.handle
    }

    fn release_external(&mut self, _handle: TextureHandle) {}

    fn memory(&self) -> MemoryReport {
        convert::memory(self.renderer.memory())
    }

    fn vector_status(&self) -> zgui_render::VectorStatus {
        convert::vector_status(self.renderer.vector_status())
    }

    fn release_cached_targets(&mut self) -> u64 {
        self.renderer.release_cached_targets()
    }

    fn release_idle_resources(&mut self) -> u64 {
        self.renderer.release_idle_resources()
    }

    fn acquire_block(&self) -> core::time::Duration {
        self.renderer.acquire_block()
    }

    fn present_held(&mut self) -> FrameOutcome {
        convert::outcome(self.renderer.present_held())
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.sink
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(self)
    }
}
