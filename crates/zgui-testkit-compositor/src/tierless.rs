//! A compositor without the compositor-driven tier.

use std::sync::Arc;

use iceblink::smallvec::SmallVec;
use iceblink::{
    Built, ClientId, CommitError, CommitReport, Compositor, CompositorBudgets,
    CompositorCapabilities, CompositorEvent, CompositorMemory, Counters, FontRegistry, Frame,
    FrameId, FrameOutcome, FrameRequest, GlyphRaster, HitResult, IdSpace, OutputConfig, OutputId,
    ScrollGesture, ScrollRouting, Transaction,
};

/// Returns `compositor` without the compositor-driven tier: it animates nothing, routes no
/// gesture and hits nothing, as the spec lets a compositor do.
///
/// A client then writes every scroll offset itself.
pub fn tierless(compositor: Box<dyn Compositor>) -> Box<dyn Compositor> {
    Box::new(Tierless(compositor))
}

/// A compositor that reports and honours no compositor-driven tier.
struct Tierless(Box<dyn Compositor>);

impl Compositor for Tierless {
    fn capabilities(&self) -> CompositorCapabilities {
        CompositorCapabilities {
            animations: false,
            scroll_input: false,
            hit_test: false,
            ..self.0.capabilities()
        }
    }

    fn register_client(&mut self) -> (ClientId, IdSpace) {
        self.0.register_client()
    }

    fn unregister_client(&mut self, client: ClientId) {
        self.0.unregister_client(client);
    }

    fn transaction(&mut self, client: ClientId) -> Transaction {
        self.0.transaction(client)
    }

    fn commit(&mut self, transaction: Transaction) -> Result<CommitReport, CommitError> {
        self.0.commit(transaction)
    }

    fn use_font_registry(&mut self, registry: Arc<dyn FontRegistry>) {
        self.0.use_font_registry(registry);
    }

    fn font_registry(&self) -> Arc<dyn FontRegistry> {
        self.0.font_registry()
    }

    fn use_glyph_raster(&mut self, raster: Arc<dyn GlyphRaster>) {
        self.0.use_glyph_raster(raster);
    }

    fn create_output(&mut self, config: OutputConfig) -> OutputId {
        self.0.create_output(config)
    }

    fn configure_output(&mut self, output: OutputId, config: OutputConfig) {
        self.0.configure_output(output, config);
    }

    fn destroy_output(&mut self, output: OutputId) {
        self.0.destroy_output(output);
    }

    fn build_frame(
        &mut self,
        output: OutputId,
        request: &FrameRequest<'_>,
        into: &mut Frame,
    ) -> Built {
        self.0.build_frame(output, request, into)
    }

    fn frame_done(&mut self, output: OutputId, frame: FrameId, outcome: &FrameOutcome) {
        self.0.frame_done(output, frame, outcome);
    }

    fn scroll_input(&mut self, _output: OutputId, _gesture: ScrollGesture) -> ScrollRouting {
        ScrollRouting::Unhandled
    }

    fn hit_test(&self, _output: OutputId, _point: [f32; 2]) -> SmallVec<[HitResult; 4]> {
        SmallVec::new()
    }

    fn drain_events(&mut self, out: &mut Vec<CompositorEvent>) {
        self.0.drain_events(out);
    }

    fn set_verification(&mut self, on: bool) {
        self.0.set_verification(on);
    }

    fn counters(&self) -> &Counters {
        self.0.counters()
    }

    fn memory(&self) -> CompositorMemory {
        self.0.memory()
    }

    fn set_budgets(&mut self, budgets: CompositorBudgets) {
        self.0.set_budgets(budgets);
    }

    fn budgets(&self) -> CompositorBudgets {
        self.0.budgets()
    }

    fn trim(&mut self, to_bytes: u64) -> u64 {
        self.0.trim(to_bytes)
    }

    fn forget_caches(&mut self) -> u64 {
        self.0.forget_caches()
    }

    fn extra_stats(&self, out: &mut dyn FnMut(&'static str, u64)) {
        self.0.extra_stats(out);
    }
}
