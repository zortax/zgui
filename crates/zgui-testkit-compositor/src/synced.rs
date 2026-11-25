//! A threaded driver that a test reads like one on its own thread.

use std::sync::Arc;

use iceblink::smallvec::SmallVec;
use iceblink::{
    ClientEvent, ClientId, CommitId, CompositorBudgets, CompositorCapabilities, CompositorClient,
    CompositorMemory, DriverPacing, DriverStats, EventWaker, FontRegistry, FrameClock,
    FrameObserver, HitResult, IdSpace, OutputConfig, OutputId, OutputStatus, OutputWatch,
    RendererFactory, RendererTask, ScrollGesture, ScrollId, ScrollRouting, ScrollWatch,
    SubpixelOrder, Transaction,
};

/// A driver that waits after every step until the frames of the step are drawn and published.
///
/// It wakes no client, since a client reads every answer of a step once the step returns.
pub(crate) struct Synced<C>(pub(crate) C);

impl<C: CompositorClient> CompositorClient for Synced<C> {
    fn capabilities(&self) -> CompositorCapabilities {
        self.0.capabilities()
    }

    fn pacing(&self) -> DriverPacing {
        self.0.pacing()
    }

    fn font_registry(&self) -> Arc<dyn FontRegistry> {
        self.0.font_registry()
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

    fn commit(&mut self, transaction: Transaction) -> CommitId {
        self.0.commit(transaction)
    }

    fn set_verification(&mut self, on: bool) {
        self.0.set_verification(on);
    }

    fn set_budgets(&mut self, budgets: CompositorBudgets) {
        self.0.set_budgets(budgets);
    }

    fn create_output(&mut self, config: OutputConfig, renderer: RendererFactory) -> OutputId {
        self.0.create_output(config, renderer)
    }

    fn configure_output(&mut self, output: OutputId, config: OutputConfig) {
        self.0.configure_output(output, config);
    }

    fn set_subpixel_order(&mut self, output: OutputId, order: Option<SubpixelOrder>) {
        self.0.set_subpixel_order(output, order);
    }

    fn set_refresh(&mut self, output: OutputId, refresh_ns: Option<u64>) {
        self.0.set_refresh(output, refresh_ns);
    }

    fn report_presented(&mut self, output: OutputId, presented_ns: u64) {
        self.0.report_presented(output, presented_ns);
    }

    fn destroy_output(&mut self, output: OutputId) {
        self.0.destroy_output(output);
    }

    fn request_frame(&mut self, output: OutputId) {
        self.0.request_frame(output);
    }

    fn scroll_input(
        &mut self,
        client: ClientId,
        output: OutputId,
        gesture: ScrollGesture,
        predicted: ScrollRouting,
    ) {
        self.0.scroll_input(client, output, gesture, predicted);
    }

    fn hit_test(&mut self, output: OutputId, point: [f32; 2]) -> SmallVec<[HitResult; 4]> {
        self.0.hit_test(output, point)
    }

    fn watch_scroll(&mut self, client: ClientId, node: ScrollId, watch: Option<ScrollWatch>) {
        self.0.watch_scroll(client, node, watch);
    }

    fn watch_animations(&mut self, client: ClientId, on: bool) {
        self.0.watch_animations(client, on);
    }

    fn watch_frames(&mut self, client: ClientId, output: OutputId, on: bool) {
        self.0.watch_frames(client, output, on);
    }

    fn watch_output(&mut self, client: ClientId, output: OutputId, watch: OutputWatch) {
        self.0.watch_output(client, output, watch);
    }

    fn observe_frames(&mut self, output: OutputId, observer: Option<FrameObserver>) {
        self.0.observe_frames(output, observer);
    }

    fn with_renderer(&mut self, output: OutputId, task: RendererTask) {
        self.0.with_renderer(output, task);
    }

    fn release_cached_targets(&mut self, output: OutputId) {
        self.0.release_cached_targets(output);
    }

    fn release_idle_resources(&mut self, output: OutputId) {
        self.0.release_idle_resources(output);
    }

    fn set_waker(&mut self, _client: ClientId, _waker: Option<EventWaker>) {
        // Every step answers before it returns, as in process, so no answer needs a wake.
    }

    fn drain_events(&mut self, client: ClientId, out: &mut Vec<ClientEvent>) {
        self.0.drain_events(client, out);
    }

    fn stats(&self) -> DriverStats {
        self.0.stats()
    }

    fn memory(&self) -> CompositorMemory {
        self.0.memory()
    }

    fn output_status(&self, output: OutputId) -> Option<OutputStatus> {
        self.0.output_status(output)
    }

    fn extra_stats(&self, out: &mut dyn FnMut(&'static str, u64)) {
        self.0.extra_stats(out);
    }

    fn output_extra_stats(&self, output: OutputId, out: &mut dyn FnMut(&'static str, u64)) {
        self.0.output_extra_stats(output, out);
    }

    fn trim(&mut self, to_bytes: u64) {
        self.0.trim(to_bytes);
    }

    fn forget_caches(&mut self) {
        self.0.forget_caches();
    }

    fn step(&mut self, clock: FrameClock) {
        self.0.step(clock);
        self.0.sync();
    }

    fn pump(&mut self) {
        self.0.pump();
    }

    fn sync(&mut self) {
        self.0.sync();
    }
}
