//! One window's client of the shared driver.

use compositor_api::{
    ClientEvent, CommitId, CompositorCapabilities, DriverPacing, DriverStats, EventWaker,
    FrameObserver, OutputConfig, OutputStatus, OutputWatch, RendererFactory, RendererTask,
    ScrollWatch,
};
use compositor_types::{
    ClientId, FrameClock, OutputId, RenderTarget, ScrollGesture, ScrollId, ScrollRouting,
    SubpixelOrder, Transaction,
};

use crate::compositor::{Answers, Shared};

/// A client of the shared driver and the output it draws into.
///
/// Dropping it destroys the output and unregisters the client, which removes everything the
/// client created.
pub(crate) struct Client {
    shared: Shared,
    client: ClientId,
    output: OutputId,
    /// The function the client's waking events call, set again on every registration.
    waker: Option<EventWaker>,
    /// The buffer the driver's events are drained into, kept for its allocation.
    events: Vec<ClientEvent>,
    /// Whether the output presented a frame since it was created.
    shown: bool,
    /// Whether a commit was queued since the last frame drawn for the output.
    awaiting: bool,
    /// The clock the driver was last stepped with, for a stepped driver.
    stepped: std::cell::Cell<Option<FrameClock>>,
}

impl Client {
    /// Registers a client of `shared` with an output configured for `target`, whose renderer
    /// `renderer` builds.
    ///
    /// A driver thread builds the renderer after the call returns. A renderer that could not be
    /// built is reported by the answers.
    pub(crate) fn open(shared: Shared, target: RenderTarget, renderer: RendererFactory) -> Self {
        let (client, output) = {
            let mut driver = shared.driver.borrow_mut();
            let (client, _ids) = driver.register_client();
            let output = driver.create_output(config(target), renderer);
            (client, output)
        };
        let opened = Self {
            shared,
            client,
            output,
            waker: None,
            events: Vec::new(),
            shown: false,
            awaiting: false,
            stepped: std::cell::Cell::new(None),
        };
        opened.watch();
        opened
    }

    /// Watches the client's output: every frame drawn, its first presented one, and its
    /// presenter state.
    fn watch(&self) {
        let mut driver = self.shared.driver.borrow_mut();
        driver.watch_frames(self.client, self.output, true);
        driver.watch_output(
            self.client,
            self.output,
            OutputWatch {
                shown: !self.shown,
                presenter: true,
            },
        );
    }

    /// Returns the output the client draws into.
    pub(crate) fn output(&self) -> OutputId {
        self.output
    }

    /// Returns an empty transaction of the client.
    pub(crate) fn transaction(&self) -> Transaction {
        self.shared.driver.borrow_mut().transaction(self.client)
    }

    /// Returns who runs the driver's frames.
    pub(crate) fn pacing(&self) -> DriverPacing {
        self.shared.driver.borrow().pacing()
    }

    /// Returns what the compositor can do.
    pub(crate) fn capabilities(&self) -> CompositorCapabilities {
        self.shared.driver.borrow().capabilities()
    }

    /// Returns the newest snapshot of the output and its renderer.
    pub(crate) fn status(&self) -> Option<OutputStatus> {
        self.shared.driver.borrow().output_status(self.output)
    }

    /// Returns when the driver latches its next frame on the frame clock, or `None` when no output
    /// wants one.
    ///
    /// A stepped driver on a thread of its own may not have published the last step it was given.
    /// Its snapshot is read as follows:
    ///
    /// - If the snapshot predates that step, a frame is due one interval after the step at the
    ///   latest, or earlier when the snapshot has one due then.
    /// - If it has a frame due by that step, the frame is due one interval after the step.
    /// - Otherwise the frame is due when the snapshot says.
    pub(crate) fn next_due(&self) -> Option<u64> {
        let stats = self.stats();
        let Some(clock) = self.stepped.get() else {
            return stats.next_due_ns;
        };
        let next = clock.now_ns + clock.interval_ns.max(1);
        // A step can only bring the next frame forward to one interval after it.
        if stats.steps < self.shared.steps.get() {
            return Some(
                stats
                    .next_due_ns
                    .filter(|due| *due > clock.now_ns)
                    .map_or(next, |due| due.min(next)),
            );
        }
        stats
            .next_due_ns
            .map(|due| if due <= clock.now_ns { next } else { due })
    }

    /// Returns the latch and display time of the frame a commit queued now goes into, on the frame
    /// clock, or `None` for a stepped driver.
    ///
    /// A snapshot whose latch has passed is moved on by whole intervals of `interval`.
    pub(crate) fn commit_frame(&self, interval: std::time::Duration) -> Option<(u64, u64)> {
        let stats = self.stats();
        let (due, display) = (stats.commit_due_ns?, stats.commit_display_ns?);
        let now = compositor_api::now_ns();
        let interval = u64::try_from(interval.as_nanos())
            .unwrap_or(u64::MAX)
            .max(1);
        let steps = now.saturating_sub(due).div_ceil(interval);
        Some((due + steps * interval, display + steps * interval))
    }

    /// Returns the newest snapshot of the driver and its compositor.
    pub(crate) fn stats(&self) -> DriverStats {
        self.shared.driver.borrow().stats()
    }

    /// Returns the memory the compositor holds.
    pub(crate) fn memory(&self) -> compositor_api::CompositorMemory {
        self.shared.driver.borrow().memory()
    }

    /// Calls `out` with each implementation-specific counter of the compositor.
    pub(crate) fn extra_stats(&self, out: &mut dyn FnMut(&'static str, u64)) {
        self.shared.driver.borrow().extra_stats(out);
    }

    /// Calls `out` with each implementation-specific counter of the output's renderer.
    pub(crate) fn renderer_stats(&self, out: &mut dyn FnMut(&'static str, u64)) {
        self.shared
            .driver
            .borrow()
            .output_extra_stats(self.output, out);
    }

    /// Queues a commit of `transaction`.
    pub(crate) fn commit(&mut self, transaction: Transaction) -> CommitId {
        self.awaiting = true;
        self.shared.driver.borrow_mut().commit(transaction)
    }

    /// Queues one step of a scroll gesture over the output, which the window predicts the
    /// compositor routes to `predicted`, or leaves unhandled for `None`.
    pub(crate) fn scroll_input(&self, gesture: ScrollGesture, predicted: Option<ScrollId>) {
        let predicted = match predicted {
            Some(node) => ScrollRouting::Handled {
                node,
                client: self.client,
            },
            None => ScrollRouting::Unhandled,
        };
        self.shared
            .driver
            .borrow_mut()
            .scroll_input(self.client, self.output, gesture, predicted);
    }

    /// Sets the motion of `node` that wakes the window, or removes it with `None`.
    pub(crate) fn watch_scroll(&self, node: ScrollId, watch: Option<ScrollWatch>) {
        self.shared
            .driver
            .borrow_mut()
            .watch_scroll(self.client, node, watch);
    }

    /// Sets the function the client's waking events call.
    pub(crate) fn set_waker(&mut self, waker: EventWaker) {
        self.shared
            .driver
            .borrow_mut()
            .set_waker(self.client, Some(waker.clone()));
        self.waker = Some(waker);
    }

    /// Sets whether the compositor checks the damage commits declare.
    pub(crate) fn set_verification(&self, on: bool) {
        self.shared.driver.borrow_mut().set_verification(on);
    }

    /// Sets the function that sees every frame built for the output before it is drawn, or
    /// removes it with `None`.
    pub(crate) fn observe_frames(&self, observer: Option<FrameObserver>) {
        self.shared
            .driver
            .borrow_mut()
            .observe_frames(self.output, observer);
    }

    /// Reports that a frame of the output reached the display at `presented_ns` on the frame clock.
    pub(crate) fn report_presented(&self, presented_ns: u64) {
        self.shared
            .driver
            .borrow_mut()
            .report_presented(self.output, presented_ns);
    }

    /// Waits until the driver has published the output's renderer, if it has not yet.
    pub(crate) fn await_renderer(&self) {
        if self.status().is_none() {
            self.shared.driver.borrow_mut().sync();
        }
    }

    /// Reports whether the output presented a frame since it was created.
    pub(crate) const fn shown(&self) -> bool {
        self.shown
    }

    /// Takes what the driver queued for the client since the last call.
    pub(crate) fn answers(&mut self) -> Answers {
        self.events.clear();
        self.shared
            .driver
            .borrow_mut()
            .drain_events(self.client, &mut self.events);
        let mut answers = Answers::of(&mut self.events);
        self.shown |= answers.shown;
        // A stepped driver built every frame it owed in the step before, so a commit no frame
        // drew changed nothing.
        let stepped = self.pacing() == DriverPacing::Stepped;
        answers.awaiting = self.awaiting && answers.drawn.is_empty() && !stepped;
        self.awaiting = answers.awaiting;
        answers
    }

    /// Registers the client again with nothing, so the next transaction has to create everything
    /// the client draws again.
    ///
    /// The client's watches go with its old registration, and its waker is set again.
    pub(crate) fn restart(&mut self) {
        let mut driver = self.shared.driver.borrow_mut();
        driver.unregister_client(self.client);
        let (client, _ids) = driver.register_client();
        self.client = client;
        driver.set_waker(client, self.waker.clone());
        drop(driver);
        self.watch();
    }

    /// Queues `task` on the output's renderer.
    pub(crate) fn with_renderer(&self, task: RendererTask) {
        self.shared
            .driver
            .borrow_mut()
            .with_renderer(self.output, task);
    }

    /// Runs `inspect` on the output's renderer and returns its answer, waiting for the driver.
    ///
    /// Returns `None` when the output has no renderer.
    pub(crate) fn inspect<T: Send + 'static>(
        &self,
        inspect: impl FnOnce(&mut dyn compositor_api::Renderer) -> T + Send + 'static,
    ) -> Option<T> {
        let answer = std::sync::Arc::new(std::sync::Mutex::new(None));
        let answered = std::sync::Arc::clone(&answer);
        let mut driver = self.shared.driver.borrow_mut();
        driver.with_renderer(
            self.output,
            Box::new(move |renderer| {
                let value = inspect(renderer);
                *answered
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(value);
            }),
        );
        driver.sync();
        drop(driver);
        answer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    /// Queues the release of the pooled targets the output's renderer keeps.
    pub(crate) fn release_cached_targets(&self) {
        self.shared
            .driver
            .borrow_mut()
            .release_cached_targets(self.output);
    }

    /// Queues the release of what the output's renderer holds for frames it is not drawing.
    pub(crate) fn release_idle_resources(&self) {
        self.shared
            .driver
            .borrow_mut()
            .release_idle_resources(self.output);
    }

    /// Points the output at a new or resized target.
    pub(crate) fn configure(&self, target: RenderTarget) {
        self.shared
            .driver
            .borrow_mut()
            .configure_output(self.output, config(target));
    }

    /// Sets the stripe order the output draws per-channel text for, or `None` for none.
    pub(crate) fn set_subpixel_order(&self, order: Option<SubpixelOrder>) {
        self.shared
            .driver
            .borrow_mut()
            .set_subpixel_order(self.output, order);
    }

    /// Sets the refresh interval of the display the output is shown on, or `None` when the
    /// platform reports none.
    pub(crate) fn set_refresh(&self, refresh_ns: Option<u64>) {
        self.shared
            .driver
            .borrow_mut()
            .set_refresh(self.output, refresh_ns);
    }

    /// Asks for a frame of the output, such as when the platform is ready for the one it holds.
    pub(crate) fn request_frame(&self) {
        self.shared.driver.borrow_mut().request_frame(self.output);
    }

    /// Runs the frames that are due on the window's thread, at `clock`.
    ///
    /// A stepped driver builds the frames due by `clock`, and a pumped one the frames that
    /// latched. A self-paced driver builds its frames on its own.
    pub(crate) fn run_due(&self, clock: FrameClock) {
        let mut driver = self.shared.driver.borrow_mut();
        match driver.pacing() {
            DriverPacing::Stepped => {
                driver.step(clock);
                self.stepped.set(Some(clock));
                self.shared.steps.set(self.shared.steps.get() + 1);
            }
            DriverPacing::Pumped => driver.pump(),
            DriverPacing::SelfPaced => {}
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let mut driver = self.shared.driver.borrow_mut();
        driver.destroy_output(self.output);
        driver.unregister_client(self.client);
    }
}

/// Returns the output configuration that matches `target`.
fn config(target: RenderTarget) -> OutputConfig {
    OutputConfig {
        size: target.size,
        scale: target.scale,
        opaque: target.opaque,
    }
}
