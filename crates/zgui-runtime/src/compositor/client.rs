//! One window's client of the shared compositor.

use compositor_api::{CommitReport, OutputConfig};
use compositor_types::{
    ClientId, CommitError, CompositorEvent, Frame, FrameCounters, OutputId, RenderTarget,
    Transaction,
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
}

impl Client {
    /// Registers a client of `compositor` with an output configured for `target`.
    pub(crate) fn open(compositor: Shared, target: RenderTarget) -> Self {
        let (client, output) = {
            let mut held = compositor.borrow_mut();
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
        }
    }

    /// Returns the output the client draws into.
    pub(crate) fn output(&self) -> OutputId {
        self.output
    }

    /// Returns an empty transaction of the client.
    pub(crate) fn transaction(&self) -> Transaction {
        self.compositor.borrow_mut().transaction(self.client)
    }

    /// Commits `transaction`, and discards the events the compositor reported since the last commit.
    ///
    /// A rejected transaction is logged, and the client is registered again with nothing, so the
    /// next transaction has to create everything the client draws again.
    ///
    /// # Errors
    ///
    /// Returns the compositor's reason when it rejects the transaction.
    pub(crate) fn commit(&mut self, transaction: Transaction) -> Result<CommitReport, CommitError> {
        let mut compositor = self.compositor.borrow_mut();
        // Nothing acts on the compositor's events yet, so they are dropped rather than left to
        // accumulate in the compositor.
        compositor.drain_events(&mut self.events);
        self.events.clear();
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
        let mut compositor = self.compositor.borrow_mut();
        compositor.unregister_client(self.client);
        let (client, _ids) = compositor.register_client();
        self.client = client;
    }

    /// Returns the compositor the client draws through.
    pub(crate) fn compositor(&self) -> std::cell::Ref<'_, Box<dyn compositor_api::Compositor>> {
        self.compositor.borrow()
    }

    /// Points the output at a new or resized target.
    pub(crate) fn configure(&self, target: RenderTarget) {
        self.compositor
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
        let mut compositor = self.compositor.borrow_mut();
        compositor.destroy_output(self.output);
        compositor.unregister_client(self.client);
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
