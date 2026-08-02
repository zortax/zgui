//! What an event is, at the type level.

use zgui_vocab::{EventKind, Payload};

/// How a context presents an event's payload.
///
/// Separate from [`EventType`] because the untyped context has to have a payload view too, and
/// there is no single [`EventKind`] it could name — the whole point of the untyped form is that
/// the kind is not known at the type level.
pub trait EventView: Copy + 'static {
    /// What a handler registered this way sees when it dereferences its context.
    type Payload: ?Sized;

    /// Narrows a dispatched payload to this view.
    ///
    /// # Panics
    ///
    /// If the payload is not the kind this view is for. A registration and a dispatch agree on
    /// the kind by construction, so reaching this is a bug in a backend or a dispatcher rather
    /// than a case a handler has to think about.
    fn view(payload: &Payload) -> &Self::Payload;
}

/// One event's name, payload type and runtime kind.
///
/// Each constant in [`events`](crate::events) has its own type implementing this, which is what
/// makes a handler's argument type inferable from the constant alone.
///
/// ```
/// use zgui_view::events::{self, EventType};
/// use zgui_vocab::EventKind;
///
/// assert_eq!(events::CLICK.kind(), EventKind::Click);
///
/// // Two event constants have two *different* types, so `[CLICK, KEY_DOWN]` is not an array.
/// // Anything taking a set of events takes `&[EventKind]`, built with `kind`.
/// let activation = [events::CLICK.kind(), events::KEY_DOWN.kind()];
/// assert_eq!(activation.len(), 2);
/// ```
pub trait EventType: EventView {
    /// Which event this is.
    const KIND: EventKind;

    /// This event's runtime kind, as a value.
    fn kind(&self) -> EventKind {
        Self::KIND
    }
}

/// The payload view of a context that is not typed for any one event.
///
/// A handler registered against this sees the whole [`Payload`] and matches on it. This is what a
/// dispatcher holds before it re-types the context for each registration.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct AnyEvent;

impl EventView for AnyEvent {
    type Payload = Payload;

    fn view(payload: &Payload) -> &Payload {
        payload
    }
}
