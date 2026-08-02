//! What a listener is handed.

use core::cell::Cell;
use core::marker::PhantomData;
use core::ops::Deref;

use zgui_geom::{Device, DevicePx, Rect};
use zgui_vocab::{DefaultAction, EventKind, Modifiers, Payload, Phase, Propagation, Timestamp};

use crate::event::sink::EventSink;
use crate::event::view::{AnyEvent, EventType, EventView};
use crate::id::NodeId;

/// What a handler may say about the event it has just seen.
///
/// Held by the dispatcher and shared with every handler on the path, which is how one handler's
/// `stop_propagation` is visible to the dispatcher before the next one runs.
#[derive(Debug, Default)]
pub struct EventControl {
    /// How far the event should keep travelling.
    propagation: Cell<Propagation>,
    /// Whether the framework's own behaviour should still happen.
    default_action: Cell<DefaultAction>,
}

impl EventControl {
    /// A control nobody has said anything to yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// How far the event should keep travelling.
    pub fn propagation(&self) -> Propagation {
        self.propagation.get()
    }

    /// Whether the framework's own behaviour should still happen.
    pub fn default_action(&self) -> DefaultAction {
        self.default_action.get()
    }
}

/// The context delivered to a listener, parameterised by the event it was registered for.
///
/// It dereferences to the event's own payload, so a `key_down` handler writes `ev.key` and a
/// `pointer_down` handler writes `ev.position`, with no downcast and no accessor. That works
/// because the type comes from the event constant the handler was registered with, and the
/// registration and the dispatch agree on the kind by construction.
///
/// ```
/// use zgui_vocab::{Modifiers, Payload, Phase, TextEvent, Timestamp};
/// use zgui_view::events::{self, EventType, Text};
/// use zgui_view::{DiscardCommands, DocumentId, EventControl, EventCx, NodeId};
///
/// let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
/// let payload = Payload::Text(TextEvent::new("x"));
/// let control = EventControl::new();
/// let mut sink = DiscardCommands;
///
/// let mut ev: EventCx<'_, Text> = EventCx::new(
///     events::TEXT.kind(), node, node, Phase::Target, Modifiers::NONE, Timestamp::ORIGIN,
///     &payload, &control, &mut sink,
/// );
///
/// assert_eq!(ev.text.as_str(), "x"); // through the deref, with no downcast
/// ev.stop_propagation();
/// assert!(!control.propagation().continues_to_next_element());
/// ```
pub struct EventCx<'a, E: EventView = AnyEvent> {
    /// Which event this is.
    pub kind: EventKind,
    /// The node the event was aimed at.
    pub target: NodeId,
    /// The node whose listener is running.
    pub current: NodeId,
    /// Which leg of the dispatch this is.
    pub phase: Phase,
    /// Which modifier keys were held.
    pub modifiers: Modifiers,
    /// When the event happened.
    pub timestamp: Timestamp,
    /// What the event carries.
    payload: &'a Payload,
    /// What handlers have said about it so far.
    control: &'a EventControl,
    /// Where commands go.
    sink: &'a mut dyn EventSink,
    /// The current node's box as of the last completed frame.
    bounds: Option<Rect<DevicePx, Device>>,
    /// Which event this context is typed for.
    event: PhantomData<fn() -> E>,
}

impl<'a, E: EventView> EventCx<'a, E> {
    /// Assembles a context. The dispatcher calls this; a handler receives the result.
    ///
    /// The kind is the one the listener was registered under, which is what makes the payload
    /// view infallible.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: EventKind,
        target: NodeId,
        current: NodeId,
        phase: Phase,
        modifiers: Modifiers,
        timestamp: Timestamp,
        payload: &'a Payload,
        control: &'a EventControl,
        sink: &'a mut dyn EventSink,
    ) -> Self {
        Self {
            kind,
            target,
            current,
            phase,
            modifiers,
            timestamp,
            payload,
            control,
            sink,
            bounds: None,
            event: PhantomData,
        }
    }

    /// Records the current node's box, as of the last completed frame.
    #[must_use]
    pub fn with_bounds(mut self, bounds: Option<Rect<DevicePx, Device>>) -> Self {
        self.bounds = bounds;
        self
    }

    /// The whole payload, whatever kind it is.
    pub fn payload(&self) -> &Payload {
        self.payload
    }

    /// The current node's box, as of the **last completed frame**.
    ///
    /// A handler must never make layout run; the geometry it can see is the geometry that was
    /// there when the frame it is responding to was painted.
    pub fn bounds(&self) -> Option<Rect<DevicePx, Device>> {
        self.bounds
    }

    /// Asks that the event travel no further, after this element's other listeners have run.
    pub fn stop_propagation(&self) {
        let running = self.control.propagation.get();
        self.control
            .propagation
            .set(running.strongest(Propagation::Stop));
    }

    /// Asks that the event stop at once, without this element's remaining listeners running.
    pub fn stop_immediate_propagation(&self) {
        self.control.propagation.set(Propagation::StopImmediate);
    }

    /// Asks that the framework's own behaviour for this event be skipped.
    pub fn prevent_default(&self) {
        self.control.default_action.set(DefaultAction::Prevented);
    }

    /// Routes every subsequent pointer event to the current node until the button is released.
    pub fn capture_pointer(&mut self) {
        let node = self.current;
        self.sink.capture_pointer(node);
    }

    /// Ends a capture early.
    pub fn release_pointer(&mut self) {
        let node = self.current;
        self.sink.release_pointer(node);
    }

    /// Moves focus to `node`, once this dispatch has finished.
    pub fn request_focus(&mut self, node: NodeId) {
        self.sink.request_focus(node);
    }

    /// Dispatches `event` on the current node, through the ordinary path.
    ///
    /// The one line that makes a custom control keyboard-operable, and the same path an inbound
    /// accessibility action takes.
    pub fn synthesize<T: EventType>(&mut self, event: T) {
        let node = self.current;
        self.sink.synthesize(node, event.kind());
    }

    /// Re-types this context for a different view of the same payload.
    ///
    /// The dispatcher holds an untyped context and every registration knows the type it wants;
    /// this is the one conversion between them, and it is infallible for the kind the listener
    /// was registered under.
    pub fn retype<T: EventView>(&mut self) -> EventCx<'_, T> {
        EventCx {
            kind: self.kind,
            target: self.target,
            current: self.current,
            phase: self.phase,
            modifiers: self.modifiers,
            timestamp: self.timestamp,
            payload: self.payload,
            control: self.control,
            sink: &mut *self.sink,
            bounds: self.bounds,
            event: PhantomData,
        }
    }
}

impl<E: EventView> Deref for EventCx<'_, E> {
    type Target = E::Payload;

    fn deref(&self) -> &Self::Target {
        E::view(self.payload)
    }
}

#[cfg(test)]
mod tests {
    use zgui_vocab::{Modifiers, Payload, Phase, PointerEvent, Propagation, TextEvent, Timestamp};

    use super::{EventControl, EventCx};
    use crate::event::sink::DiscardCommands;
    use crate::event::view::EventType;
    use crate::events::{self, Text};
    use crate::{DocumentId, NodeId};

    fn node() -> NodeId {
        NodeId::new(DocumentId::FIRST, 1).expect("in range")
    }

    fn pointer() -> PointerEvent {
        PointerEvent::mouse(zgui_geom::Point::new(
            zgui_geom::CssPx(0.0),
            zgui_geom::CssPx(0.0),
        ))
    }

    #[test]
    fn the_typed_context_dereferences_to_the_event_payload() {
        let payload = Payload::Text(TextEvent::new("abc"));
        let control = EventControl::new();
        let mut sink = DiscardCommands;
        let ev: EventCx<'_, Text> = EventCx::new(
            events::TEXT.kind(),
            node(),
            node(),
            Phase::Target,
            Modifiers::NONE,
            Timestamp::ORIGIN,
            &payload,
            &control,
            &mut sink,
        );
        assert_eq!(ev.text.as_str(), "abc");
    }

    #[test]
    fn the_untyped_context_dereferences_to_the_whole_payload() {
        let payload = Payload::Pointer(pointer());
        let control = EventControl::new();
        let mut sink = DiscardCommands;
        let mut ev: EventCx<'_> = EventCx::new(
            zgui_vocab::EventKind::PointerDown,
            node(),
            node(),
            Phase::Target,
            Modifiers::NONE,
            Timestamp::ORIGIN,
            &payload,
            &control,
            &mut sink,
        );
        assert!(ev.as_pointer().is_some());

        // ... and re-types into the one the registration asked for, with no downcast.
        let typed = ev.retype::<crate::events::PointerDown>();
        assert_eq!(typed.position, pointer().position);
    }

    #[test]
    fn preventing_the_default_is_a_separate_answer_from_stopping_the_event() {
        let payload = Payload::Text(TextEvent::new(""));
        let control = EventControl::new();
        let mut sink = DiscardCommands;
        let ev: EventCx<'_, Text> = EventCx::new(
            events::TEXT.kind(),
            node(),
            node(),
            Phase::Target,
            Modifiers::NONE,
            Timestamp::ORIGIN,
            &payload,
            &control,
            &mut sink,
        );

        assert!(control.default_action().is_allowed());
        ev.prevent_default();
        assert!(!control.default_action().is_allowed());
        assert!(
            control.propagation().continues_to_next_element(),
            "suppressing the framework's behaviour says nothing about where the event goes"
        );
    }

    #[test]
    fn stopping_never_weakens_a_stronger_request() {
        let payload = Payload::Text(TextEvent::new(""));
        let control = EventControl::new();
        let mut sink = DiscardCommands;
        let ev: EventCx<'_, Text> = EventCx::new(
            events::TEXT.kind(),
            node(),
            node(),
            Phase::Target,
            Modifiers::NONE,
            Timestamp::ORIGIN,
            &payload,
            &control,
            &mut sink,
        );

        ev.stop_immediate_propagation();
        ev.stop_propagation();
        assert_eq!(control.propagation(), Propagation::StopImmediate);
    }
}
