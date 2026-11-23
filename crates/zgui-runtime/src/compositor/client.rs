//! One window's client of the shared driver.

use compositor_api::{
    ClientEvent, CommitId, CompositorCapabilities, DriverPacing, DriverStats, EventWaker,
    FrameObserver, OutputConfig, OutputStatus, OutputWatch, RendererFactory, RendererTask,
    ScrollWatch,
};
use compositor_types::{
    ClientId, FrameClock, GpuUnavailable, OutputId, RenderTarget, ScrollGesture, ScrollId,
    ScrollRouting, SubpixelOrder, Transaction,
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
    /// The waker the client's events wake, set again on every registration.
    waker: Option<EventWaker>,
    /// The buffer the driver's events are drained into, kept for its allocation.
    events: Vec<ClientEvent>,
    /// Whether the output presented a frame since it was created.
    shown: bool,
    /// Whether a commit was queued since the last frame drawn for the output.
    awaiting: bool,
}

impl Client {
    /// Registers a client of `shared` with an output configured for `target`, whose renderer
    /// `renderer` builds.
    ///
    /// # Errors
    ///
    /// Fails, and leaves nothing registered, when the renderer could not be built.
    pub(crate) fn open(
        shared: Shared,
        target: RenderTarget,
        renderer: RendererFactory,
    ) -> Result<Self, GpuUnavailable> {
        let (client, output) = {
            let mut driver = shared.driver.borrow_mut();
            let (client, _ids) = driver.register_client();
            let output = driver.create_output(config(target), renderer);
            (client, output)
        };
        let mut opened = Self {
            shared,
            client,
            output,
            waker: None,
            events: Vec::new(),
            shown: false,
            awaiting: false,
        };
        if opened.status().is_some_and(|status| status.failed) {
            return Err(opened.answers().failed.unwrap_or_default());
        }
        opened.watch();
        Ok(opened)
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

    /// Returns the newest snapshot of the driver and its compositor.
    pub(crate) fn stats(&self) -> DriverStats {
        self.shared.driver.borrow().stats()
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
            DriverPacing::Stepped => driver.step(clock),
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
