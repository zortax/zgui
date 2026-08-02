//! Events: how a listener is registered, what it is handed, and what it can ask for.
//!
//! Typed at the author's edge, erased at the backend's. A handler is registered against an event
//! *constant* whose type carries the payload, so the closure's argument type is inferred and a
//! misspelled event name is a compile error. What crosses the [`Dom`](crate::Dom) edge is one
//! erased closure over an untyped context, because a trait that is generic over the event is not
//! object-safe and object safety is the whole point of the seam.

mod cx;
mod handler;
mod kinds;
mod listener;
mod sink;
mod view;

pub use crate::event::cx::{EventControl, EventCx};
pub use crate::event::handler::handler;
pub use crate::event::listener::{ListenerRegistration, erase};
pub use crate::event::sink::{DiscardCommands, EventSink};
pub use crate::event::view::{AnyEvent, EventType, EventView};

/// One type and one constant per event in the vocabulary.
///
/// A listener is registered with the constant, and its handler's argument type follows from the
/// constant's type:
///
/// ```
/// use zgui_view::events;
///
/// // `ev` is an `EventCx<'_, Click>`, which dereferences to a `PointerEvent`.
/// let handler = |ev: &mut zgui_view::EventCx<'_, events::Click>| {
///     let _ = ev.position;
/// };
/// let _ = (events::CLICK, handler);
/// ```
pub mod events {
    pub use crate::event::kinds::*;
    pub use crate::event::view::{AnyEvent, EventType, EventView};
}
