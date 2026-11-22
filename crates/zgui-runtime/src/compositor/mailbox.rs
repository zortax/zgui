//! The compositor's events, held until the client they belong to takes them.

use compositor_types::{ClientId, CompositorEvent};

/// The events drained from the compositor that their clients have not taken yet.
///
/// Events come out in the order the compositor queued them.
#[derive(Debug, Default)]
pub(super) struct Mailbox {
    held: Vec<CompositorEvent>,
}

impl Mailbox {
    /// Holds `events`, leaving it empty.
    pub(super) fn hold(&mut self, events: &mut Vec<CompositorEvent>) {
        Vec::append(&mut self.held, events);
    }

    /// Moves the events of `client` into `out`, and drops those of `retired`.
    pub(super) fn take(
        &mut self,
        client: ClientId,
        retired: &[ClientId],
        out: &mut Vec<CompositorEvent>,
    ) {
        self.held.retain(|event| {
            let of = client_of(event);
            if of == client {
                out.push(event.clone());
                return false;
            }
            !retired.contains(&of)
        });
    }

    /// Drops every event of `client`.
    pub(super) fn forget(&mut self, client: ClientId) {
        self.held.retain(|event| client_of(event) != client);
    }
}

/// Returns the client `event` is reported to.
fn client_of(event: &CompositorEvent) -> ClientId {
    match *event {
        CompositorEvent::Animation { client, .. }
        | CompositorEvent::Scroll { client, .. }
        | CompositorEvent::HintViolated { client, .. }
        | CompositorEvent::DamageUndeclared { client, .. } => client,
    }
}
