//! A compositor that records the scroll gestures it is handed.

use std::sync::{Arc, Mutex};

use iceblink::smallvec::SmallVec;
use iceblink::{
    Built, ClientId, CommitError, CommitReport, Compositor, CompositorBudgets,
    CompositorCapabilities, CompositorEvent, CompositorMemory, Counters, FontRegistry, Frame,
    FrameId, FrameOutcome, FrameRequest, GlyphRaster, HitResult, IdSpace, OutputConfig, OutputId,
    ScrollGesture, ScrollRouting, Transaction,
};

/// Returns `compositor` recording every scroll gesture it is handed, and the record.
///
/// The compositor acts on each gesture as `compositor` does.
pub fn recording(compositor: Box<dyn Compositor>) -> (Box<dyn Compositor>, Gestures) {
    let gestures = Gestures::default();
    let recording = Recording {
        inner: compositor,
        gestures: gestures.clone(),
    };
    (Box::new(recording), gestures)
}

/// The scroll gestures a [`recording`] compositor was handed, readable after the compositor has
/// been handed away.
#[derive(Clone, Debug, Default)]
pub struct Gestures(Arc<Mutex<Vec<ScrollGesture>>>);

impl Gestures {
    /// Returns every gesture handed so far, oldest first.
    pub fn handed(&self) -> Vec<ScrollGesture> {
        self.0
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone()
    }

    fn push(&self, gesture: ScrollGesture) {
        self.0
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push(gesture);
    }
}

/// A compositor that records the gestures it is handed before it acts on them.
struct Recording {
    inner: Box<dyn Compositor>,
    gestures: Gestures,
}

impl Compositor for Recording {
    fn capabilities(&self) -> CompositorCapabilities {
        self.inner.capabilities()
    }

    fn register_client(&mut self) -> (ClientId, IdSpace) {
        self.inner.register_client()
    }

    fn unregister_client(&mut self, client: ClientId) {
        self.inner.unregister_client(client);
    }

    fn transaction(&mut self, client: ClientId) -> Transaction {
        self.inner.transaction(client)
    }

    fn commit(&mut self, transaction: Transaction) -> Result<CommitReport, CommitError> {
        self.inner.commit(transaction)
    }

    fn use_font_registry(&mut self, registry: Arc<dyn FontRegistry>) {
        self.inner.use_font_registry(registry);
    }

    fn font_registry(&self) -> Arc<dyn FontRegistry> {
        self.inner.font_registry()
    }

    fn use_glyph_raster(&mut self, raster: Arc<dyn GlyphRaster>) {
        self.inner.use_glyph_raster(raster);
    }

    fn create_output(&mut self, config: OutputConfig) -> OutputId {
        self.inner.create_output(config)
    }

    fn configure_output(&mut self, output: OutputId, config: OutputConfig) {
        self.inner.configure_output(output, config);
    }

    fn destroy_output(&mut self, output: OutputId) {
        self.inner.destroy_output(output);
    }

    fn build_frame(
        &mut self,
        output: OutputId,
        request: &FrameRequest<'_>,
        into: &mut Frame,
    ) -> Built {
        self.inner.build_frame(output, request, into)
    }

    fn frame_done(&mut self, output: OutputId, frame: FrameId, outcome: &FrameOutcome) {
        self.inner.frame_done(output, frame, outcome);
    }

    fn next_wake(&self, output: OutputId) -> Option<u64> {
        self.inner.next_wake(output)
    }

    fn scroll_input(&mut self, output: OutputId, gesture: ScrollGesture) -> ScrollRouting {
        self.gestures.push(gesture);
        self.inner.scroll_input(output, gesture)
    }

    fn hit_test(&self, output: OutputId, point: [f32; 2]) -> SmallVec<[HitResult; 4]> {
        self.inner.hit_test(output, point)
    }

    fn drain_events(&mut self, out: &mut Vec<CompositorEvent>) {
        self.inner.drain_events(out);
    }

    fn set_verification(&mut self, on: bool) {
        self.inner.set_verification(on);
    }

    fn counters(&self) -> &Counters {
        self.inner.counters()
    }

    fn memory(&self) -> CompositorMemory {
        self.inner.memory()
    }

    fn set_budgets(&mut self, budgets: CompositorBudgets) {
        self.inner.set_budgets(budgets);
    }

    fn budgets(&self) -> CompositorBudgets {
        self.inner.budgets()
    }

    fn trim(&mut self, to_bytes: u64) -> u64 {
        self.inner.trim(to_bytes)
    }

    fn forget_caches(&mut self) -> u64 {
        self.inner.forget_caches()
    }

    fn extra_stats(&self, out: &mut dyn FnMut(&'static str, u64)) {
        self.inner.extra_stats(out);
    }
}
