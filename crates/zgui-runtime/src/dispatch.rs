//! Calling the listeners the input system resolved.
//!
//! The split is not tidiness. Which listeners an event reaches, in which order, is a question
//! about the document and is answered below the view layer; *calling* one means building the
//! context a handler is written against, which is a view-layer type. So the input system hands
//! back a list of names — element, listener, leg — and this walks it, looks each name up in the
//! window's table of handlers, and calls.
//!
//! That split is also what makes `stop_propagation` this side's business. The list is the whole
//! order, so honouring a request to stop is a matter of where the walk ends — and there are two
//! different answers: an element's own remaining listeners still run when the event is asked to
//! stop, and do not when it is asked to stop immediately. Resolving the order again from inside
//! the walk would re-enter the document mid-dispatch, which is exactly what the mutation protocol
//! exists to prevent.

use zgui_view::{DiscardCommands, EventControl, EventCx, EventSink, NodeId};
use zgui_vocab::{DefaultAction, EventKind, Modifiers, Payload, Timestamp};

use crate::host::{Command, RuntimeHost};

/// What one dispatch produced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dispatched {
    /// How many handlers ran.
    pub called: usize,
    /// Whether the framework's own behaviour should still happen.
    pub default_allowed: bool,
}

/// The commands a handler issues, routed to the host that carries them out.
///
/// A handler runs while the document is mid-change, so nothing it asks for may take effect where
/// it is asked for.
pub struct HostSink<'a> {
    /// Where the commands go.
    host: &'a RuntimeHost,
}

impl<'a> HostSink<'a> {
    /// A sink that appends to `host`'s command queue.
    pub fn new(host: &'a RuntimeHost) -> Self {
        Self { host }
    }
}

impl EventSink for HostSink<'_> {
    fn capture_pointer(&mut self, node: NodeId) {
        self.host.issue(Command::CapturePointer(node));
    }

    fn release_pointer(&mut self, node: NodeId) {
        self.host.issue(Command::ReleasePointer(node));
    }

    fn request_focus(&mut self, node: NodeId) {
        self.host.issue(Command::Focus(Some(node)));
    }

    fn synthesize(&mut self, node: NodeId, event: EventKind) {
        self.host.issue(Command::Synthesize { node, event });
    }
}

/// Where a listener's body is found, given the identity the document handed back.
///
/// The window's own table implements this. It is a trait so that the walk below can be written
/// once and exercised against a table a test controls.
pub trait Handlers {
    /// The body registered under `id`, if it still names one.
    fn handler(&self, id: zgui_dom::side::listeners::ListenerId) -> Option<Handler>;
}

/// One erased listener body, with the event type it was registered for already forgotten.
pub type Handler = std::rc::Rc<dyn Fn(&mut EventCx<'_>)>;

/// Runs the listeners `steps` names, in the order it names them.
///
/// Stops as soon as a handler asks for propagation to end, and reports whether the framework's own
/// behaviour — focusing what was pressed, activating what was released, scrolling what was under
/// the wheel — should still happen.
///
/// Each handler is looked up **by identity at the moment it is due to run**, never by position: a
/// handler that removes a listener and adds another from inside a dispatch would otherwise have
/// the replacement run in the removed one's place.
#[allow(clippy::too_many_arguments)]
pub fn run(
    handlers: &dyn Handlers,
    steps: &[zgui_input::Step],
    kind: EventKind,
    target: Option<zgui_dom::NodeKey>,
    payload: &Payload,
    modifiers: Modifiers,
    timestamp: Timestamp,
    sink: &mut dyn EventSink,
) -> Dispatched {
    let Some(target) = target else {
        return Dispatched {
            called: 0,
            default_allowed: true,
        };
    };
    let target = zgui_view_dom::id::to_view(target);
    let control = EventControl::new();
    let mut called = 0;
    let mut previous: Option<zgui_dom::NodeKey> = None;

    for step in steps {
        // Two different questions, and asking only the first turns every `stop_propagation` into a
        // `stop_immediate_propagation`. Asking to stop means the event travels no further *after
        // this element's own listeners have run*; only asking to stop immediately cuts those off.
        // An element commonly carries two — a component's own behaviour and the application's
        // handler on the same element — and the one that stops the event must not silently delete
        // the other.
        let carries_on = if previous == Some(step.node) {
            control.propagation().continues_to_next_listener()
        } else {
            control.propagation().continues_to_next_element()
        };
        if !carries_on {
            break;
        }
        previous = Some(step.node);
        let Some(handler) = handlers.handler(step.listener) else {
            continue;
        };
        let current = zgui_view_dom::id::to_view(step.node);
        let mut cx = EventCx::<zgui_view::event::AnyEvent>::new(
            kind, target, current, step.phase, modifiers, timestamp, payload, &control, sink,
        );
        handler(&mut cx);
        called += 1;
    }

    Dispatched {
        called,
        default_allowed: control.default_action() == DefaultAction::Allowed,
    }
}

/// The same, for a dispatch whose commands nobody is going to act on.
///
/// For a synthesised event raised while the real sink is already borrowed.
pub fn run_discarding(
    handlers: &dyn Handlers,
    steps: &[zgui_input::Step],
    kind: EventKind,
    target: Option<zgui_dom::NodeKey>,
    payload: &Payload,
    modifiers: Modifiers,
    timestamp: Timestamp,
) -> Dispatched {
    let mut sink = DiscardCommands;
    run(
        handlers, steps, kind, target, payload, modifiers, timestamp, &mut sink,
    )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use zgui_dom::side::listeners::ListenerId;
    use zgui_dom::{Document, EverythingMatters, NodeKey};
    use zgui_input::Step;
    use zgui_vocab::{EventKind, Modifiers, Payload, Phase, TextEvent, Timestamp};

    use super::{Dispatched, Handler, Handlers, run};

    /// A table a test controls, and the two nodes the walk runs over.
    struct Table {
        /// What each identity resolves to right now.
        handlers: RefCell<Vec<(ListenerId, Handler)>>,
    }

    impl Table {
        /// An empty table.
        fn new() -> Self {
            Self {
                handlers: RefCell::new(Vec::new()),
            }
        }

        /// Registers `body` under `id`, replacing whatever was there.
        fn set(&self, id: u64, body: impl Fn(&mut zgui_view::EventCx<'_>) + 'static) {
            let id = ListenerId::new(id);
            let mut handlers = self.handlers.borrow_mut();
            handlers.retain(|(held, _)| *held != id);
            handlers.push((id, Rc::new(body) as Handler));
        }

        /// Forgets whatever was registered under `id`.
        fn remove(&self, id: u64) {
            self.handlers
                .borrow_mut()
                .retain(|(held, _)| *held != ListenerId::new(id));
        }
    }

    impl Handlers for Table {
        fn handler(&self, id: ListenerId) -> Option<Handler> {
            self.handlers
                .borrow()
                .iter()
                .find(|(held, _)| *held == id)
                .map(|(_, body)| Rc::clone(body))
        }
    }

    /// A document with two elements in it, and their keys.
    fn two_nodes() -> (Document, NodeKey, NodeKey) {
        let document = Document::new();
        let keys = document
            .edit(&EverythingMatters, |edit| {
                let outer = edit.create_element(zgui_interned::ElementName::new("outer"));
                let inner = edit.create_element(zgui_interned::ElementName::new("inner"));
                (
                    edit.document().store().key_of(outer),
                    edit.document().store().key_of(inner),
                )
            })
            .expect("the document is not poisoned");
        (document, keys.0, keys.1)
    }

    /// One step of a plan.
    fn step(node: NodeKey, listener: u64, phase: Phase) -> Step {
        Step {
            node,
            listener: ListenerId::new(listener),
            phase,
        }
    }

    /// Runs `steps` against `table`, with a payload nothing here reads.
    fn dispatch(table: &Table, steps: &[Step], target: NodeKey) -> Dispatched {
        let payload = Payload::Text(TextEvent::new(""));
        let mut sink = zgui_view::DiscardCommands;
        run(
            table,
            steps,
            EventKind::Text,
            Some(target),
            &payload,
            Modifiers::NONE,
            Timestamp::ORIGIN,
            &mut sink,
        )
    }

    #[test]
    fn every_step_runs_in_the_order_the_plan_named() {
        let (_document, outer, inner) = two_nodes();
        let order = Rc::new(RefCell::new(Vec::new()));
        let table = Table::new();
        for (id, name) in [(1, "outer-capture"), (2, "inner"), (3, "outer-bubble")] {
            let order = Rc::clone(&order);
            table.set(id, move |_| order.borrow_mut().push(name));
        }

        let steps = [
            step(outer, 1, Phase::Capture),
            step(inner, 2, Phase::Target),
            step(outer, 3, Phase::Bubble),
        ];
        let ran = dispatch(&table, &steps, inner);

        assert_eq!(ran.called, 3);
        assert_eq!(*order.borrow(), ["outer-capture", "inner", "outer-bubble"]);
        assert!(ran.default_allowed);
    }

    #[test]
    fn stopping_lets_this_element_finish_and_no_other_element_start() {
        // The distinction the two names carry. An element with a component's own handler and an
        // application's handler on it is the ordinary case, not a contrived one, and the handler
        // that stops the event must not delete the one beside it.
        let (_document, outer, inner) = two_nodes();
        let order = Rc::new(RefCell::new(Vec::new()));
        let table = Table::new();
        {
            let order = Rc::clone(&order);
            table.set(1, move |ev| {
                order.borrow_mut().push("inner-first");
                ev.stop_propagation();
            });
        }
        {
            let order = Rc::clone(&order);
            table.set(2, move |_| order.borrow_mut().push("inner-second"));
        }
        {
            let order = Rc::clone(&order);
            table.set(3, move |_| order.borrow_mut().push("outer"));
        }

        let steps = [
            step(inner, 1, Phase::Target),
            step(inner, 2, Phase::Target),
            step(outer, 3, Phase::Bubble),
        ];
        let ran = dispatch(&table, &steps, inner);

        assert_eq!(
            *order.borrow(),
            ["inner-first", "inner-second"],
            "stopping is not stopping immediately"
        );
        assert_eq!(ran.called, 2);
    }

    #[test]
    fn stopping_immediately_stops_before_this_element_finishes() {
        let (_document, outer, inner) = two_nodes();
        let order = Rc::new(RefCell::new(Vec::new()));
        let table = Table::new();
        {
            let order = Rc::clone(&order);
            table.set(1, move |ev| {
                order.borrow_mut().push("inner-first");
                ev.stop_immediate_propagation();
            });
        }
        {
            let order = Rc::clone(&order);
            table.set(2, move |_| order.borrow_mut().push("inner-second"));
        }
        {
            let order = Rc::clone(&order);
            table.set(3, move |_| order.borrow_mut().push("outer"));
        }

        let steps = [
            step(inner, 1, Phase::Target),
            step(inner, 2, Phase::Target),
            step(outer, 3, Phase::Bubble),
        ];
        let ran = dispatch(&table, &steps, inner);

        assert_eq!(*order.borrow(), ["inner-first"]);
        assert_eq!(ran.called, 1);
    }

    #[test]
    fn a_listener_removed_mid_dispatch_does_not_run() {
        // Each identity is resolved at the moment it is due, never at the moment the plan was
        // made: a handler that removes another one has removed it, and a handler that replaces one
        // has the replacement run rather than the original.
        let (_document, _outer, inner) = two_nodes();
        let table = Rc::new(Table::new());
        let ran = Rc::new(RefCell::new(Vec::new()));
        {
            let inside = Rc::clone(&table);
            let seen = Rc::clone(&ran);
            let seen_replacement = Rc::clone(&ran);
            table.set(1, move |_| {
                seen.borrow_mut().push("first");
                inside.remove(2);
                let seen = Rc::clone(&seen_replacement);
                inside.set(3, move |_| seen.borrow_mut().push("replacement"));
            });
        }
        {
            let seen = Rc::clone(&ran);
            table.set(2, move |_| seen.borrow_mut().push("removed"));
        }
        {
            let seen = Rc::clone(&ran);
            table.set(3, move |_| seen.borrow_mut().push("original"));
        }

        let steps = [
            step(inner, 1, Phase::Target),
            step(inner, 2, Phase::Target),
            step(inner, 3, Phase::Target),
        ];
        let dispatched = dispatch(table.as_ref(), &steps, inner);

        assert_eq!(*ran.borrow(), ["first", "replacement"]);
        assert_eq!(
            dispatched.called, 2,
            "the removed listener was still called"
        );
    }

    #[test]
    fn preventing_the_default_says_nothing_about_where_the_event_goes() {
        let (_document, outer, inner) = two_nodes();
        let table = Table::new();
        table.set(1, |ev| ev.prevent_default());
        let reached = Rc::new(std::cell::Cell::new(false));
        {
            let reached = Rc::clone(&reached);
            table.set(2, move |_| reached.set(true));
        }

        let steps = [step(inner, 1, Phase::Target), step(outer, 2, Phase::Bubble)];
        let ran = dispatch(&table, &steps, inner);

        assert!(reached.get(), "preventing the default stopped the walk");
        assert!(!ran.default_allowed);
    }

    #[test]
    fn an_event_that_hit_nothing_runs_nothing_and_allows_the_default() {
        let (_document, _outer, inner) = two_nodes();
        let table = Table::new();
        table.set(1, |_| panic!("a plan aimed at nothing must run no handler"));
        let payload = Payload::Text(TextEvent::new(""));
        let mut sink = zgui_view::DiscardCommands;
        let ran = run(
            &table,
            &[step(inner, 1, Phase::Target)],
            EventKind::Text,
            None,
            &payload,
            Modifiers::NONE,
            Timestamp::ORIGIN,
            &mut sink,
        );
        assert_eq!(ran.called, 0);
        assert!(ran.default_allowed);
    }
}
