//! One window's client of the shared compositor.

use compositor_api::{CommitReport, CompositorCapabilities, OutputConfig};
use compositor_types::{
    ClientId, CommitError, CompositorEvent, Frame, FrameCounters, OutputId, RenderTarget,
    ScrollGesture, ScrollRouting, Transaction,
};

use crate::compositor::Shared;

/// A client of the shared compositor and the output it draws into.
///
/// Dropping it destroys the output and unregisters the client, which removes everything the
/// client created.
pub(crate) struct Client {
    pub(super) compositor: Shared,
    client: ClientId,
    pub(super) output: OutputId,
    /// The frame the output's frames are built into, kept for its allocations.
    pub(super) frame: Frame,
    /// The counters of the last frame built, until they are taken.
    pub(super) built: Option<FrameCounters>,
    /// The buffer the compositor's events are drained into, kept for its allocation.
    events: Vec<CompositorEvent>,
    /// The ids the client was registered under before it was registered again.
    retired: Vec<ClientId>,
}

impl Client {
    /// Registers a client of `compositor` with an output configured for `target`.
    pub(crate) fn open(compositor: Shared, target: RenderTarget) -> Self {
        let (client, output) = {
            let mut held = compositor.compositor.borrow_mut();
            let (client, _ids) = held.register_client();
            (client, held.create_output(config(target)))
        };
        Self {
            compositor,
            client,
            output,
            frame: Frame::default(),
            built: None,
            events: Vec::new(),
            retired: Vec::new(),
        }
    }

    /// Returns the output the client draws into.
    pub(crate) fn output(&self) -> OutputId {
        self.output
    }

    /// Returns an empty transaction of the client.
    pub(crate) fn transaction(&self) -> Transaction {
        self.compositor
            .compositor
            .borrow_mut()
            .transaction(self.client)
    }

    /// Returns what the compositor can do.
    pub(crate) fn capabilities(&self) -> CompositorCapabilities {
        self.compositor.compositor.borrow().capabilities()
    }

    /// Hands one step of a scroll gesture to the compositor, which routes it to a scroll node.
    pub(crate) fn scroll_input(&self, gesture: ScrollGesture) -> ScrollRouting {
        self.compositor
            .compositor
            .borrow_mut()
            .scroll_input(self.output, gesture)
    }

    /// Returns the client's frontmost item under the device point `at`, as the compositor shows it.
    pub(crate) fn hit_test(&self, at: [f32; 2]) -> Option<compositor_types::HitResult> {
        let client = self.client;
        self.compositor
            .compositor
            .borrow()
            .hit_test(self.output, at)
            .into_iter()
            .find(|hit| hit.client == client)
    }

    /// Moves the events the compositor reported for this client into `out`, oldest first.
    pub(crate) fn take_events(&mut self, out: &mut Vec<CompositorEvent>) {
        self.compositor
            .compositor
            .borrow_mut()
            .drain_events(&mut self.events);
        let mut mailbox = self.compositor.mailbox.borrow_mut();
        mailbox.hold(&mut self.events);
        mailbox.take(self.client, &self.retired, out);
    }

    /// Commits `transaction`.
    ///
    /// A rejected transaction is logged, and the client is registered again with nothing, so the
    /// next transaction has to create everything the client draws again.
    ///
    /// # Errors
    ///
    /// Returns the compositor's reason when it rejects the transaction.
    pub(crate) fn commit(&mut self, transaction: Transaction) -> Result<CommitReport, CommitError> {
        let mut compositor = self.compositor.compositor.borrow_mut();
        let error = match compositor.commit(transaction) {
            Ok(report) => return Ok(report),
            Err(error) => error,
        };
        tracing::error!(target: "zgui::compositor", ?error, "a transaction was rejected");
        drop(compositor);

        self.restart();
        Err(error)
    }

    /// Registers the client again with nothing, so the next transaction has to create everything
    /// the client draws again.
    pub(crate) fn restart(&mut self) {
        let mut compositor = self.compositor.compositor.borrow_mut();
        compositor.unregister_client(self.client);
        self.retired.push(self.client);
        let (client, _ids) = compositor.register_client();
        self.client = client;
    }

    /// Returns the compositor the client draws through.
    pub(crate) fn compositor(&self) -> std::cell::Ref<'_, Box<dyn compositor_api::Compositor>> {
        self.compositor.compositor.borrow()
    }

    /// Points the output at a new or resized target.
    pub(crate) fn configure(&self, target: RenderTarget) {
        self.compositor
            .compositor
            .borrow_mut()
            .configure_output(self.output, config(target));
    }

    /// Returns the counters of the frame built since the last call, if one was built.
    pub(crate) fn take_built(&mut self) -> Option<FrameCounters> {
        self.built.take()
    }

    /// Returns the frame most recently built for the output.
    pub(crate) fn frame(&self) -> &Frame {
        &self.frame
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let mut compositor = self.compositor.compositor.borrow_mut();
        compositor.destroy_output(self.output);
        compositor.unregister_client(self.client);
        let mut mailbox = self.compositor.mailbox.borrow_mut();
        mailbox.forget(self.client);
        for retired in &self.retired {
            mailbox.forget(*retired);
        }
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
