//! Binding a listener to a name before it is attached to an element.

use crate::event::cx::EventCx;
use crate::event::view::EventType;

/// Names a handler, so that it can be written apart from the element it is attached to.
///
/// A handler written inline needs nothing: `on:click=move |ev| …` compiles because the element
/// builder tells the compiler what the closure's argument is before the closure is read. A handler
/// bound to a name first has no such context — its argument type is decided where it is written,
/// which is before anything has said what it will be used for — and the mismatch surfaces at the
/// element as *implementation of `Fn` is not general enough*, four times over.
///
/// This is what gives it the context. The event constant fixes the payload type, so the closure's
/// argument needs no annotation and the binding is usable anywhere a handler is:
///
/// ```
/// use zgui_view::events::{self, Click};
/// use zgui_view::{EventCx, handler};
///
/// let greet = handler(events::CLICK, |ev: &mut EventCx<'_, Click>| {
///     let _ = ev.position;
/// });
/// let _also_fine = handler(events::CLICK, |_| {});
/// # let _ = greet;
/// ```
///
/// It returns the handler unchanged and costs nothing at run time: the whole of it is the type the
/// compiler now has for the closure.
///
/// The alternative, if the extra call is unwanted, is to annotate the binding's own argument —
/// `let greet = |ev: &mut EventCx<'_, Click>| …` — which says the same thing in more characters and
/// requires naming the event's type as well as its constant.
pub fn handler<E, F>(event: E, handler: F) -> F
where
    E: EventType,
    F: Fn(&mut EventCx<'_, E>) + 'static,
{
    let _ = event;
    handler
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use std::rc::Rc;

    use zgui_vocab::{Modifiers, Payload, Phase, TextEvent, Timestamp};

    use super::handler;
    use crate::event::cx::{EventControl, EventCx};
    use crate::event::sink::DiscardCommands;
    use crate::event::view::EventType;
    use crate::events::{self, Text};
    use crate::{DocumentId, NodeId};

    /// Whatever a named handler is handed to, the argument it takes is the event's own.
    ///
    /// The compile-time half of this is the whole point and it cannot be asserted at run time: if
    /// the binding below did not carry a usable type, this file would not build. What is asserted
    /// here is the other half — that the returned handler is the one that was passed, and still
    /// runs — so the case cannot pass by returning something that never fires.
    #[test]
    fn a_named_handler_is_the_handler_that_was_named() {
        let seen = Rc::new(Cell::new(0));
        let counted = Rc::clone(&seen);
        let named = handler(events::TEXT, move |ev: &mut EventCx<'_, Text>| {
            counted.set(counted.get() + ev.text.as_str().len());
        });

        let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
        let payload = Payload::Text(TextEvent::new("abc"));
        let control = EventControl::new();
        let mut sink = DiscardCommands;
        let mut cx: EventCx<'_, Text> = EventCx::new(
            events::TEXT.kind(),
            node,
            node,
            Phase::Target,
            Modifiers::NONE,
            Timestamp::ORIGIN,
            &payload,
            &control,
            &mut sink,
        );
        named(&mut cx);
        assert_eq!(
            seen.get(),
            3,
            "the named handler ran, over the real payload"
        );
    }

    /// The unannotated form, which is the one the diagnostic used to reject.
    ///
    /// Written as a binding and used afterwards, exactly as an application writes a handler it
    /// wants to name. Without the constructor this does not compile at all.
    #[test]
    fn an_unannotated_binding_is_usable_where_a_handler_is() {
        let ran = Rc::new(Cell::new(false));
        let flag = Rc::clone(&ran);
        let named = handler(events::TEXT, move |_| flag.set(true));

        let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
        let payload = Payload::Text(TextEvent::new("x"));
        let control = EventControl::new();
        let mut sink = DiscardCommands;
        let mut cx = EventCx::new(
            events::TEXT.kind(),
            node,
            node,
            Phase::Target,
            Modifiers::NONE,
            Timestamp::ORIGIN,
            &payload,
            &control,
            &mut sink,
        );
        named(&mut cx);
        assert!(ran.get());
    }
}
