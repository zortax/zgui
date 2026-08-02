//! Registering a typed handler against an untyped seam.

use std::rc::Rc;

use zgui_reactive::Owner;
use zgui_vocab::{EventKind, ListenerOptions};

use crate::dom::{Dom, ListenerId};
use crate::event::cx::EventCx;
use crate::event::view::EventType;
use crate::id::NodeId;

/// Wraps a typed handler in the erased one the backend takes.
///
/// The conversion inside is infallible *because* the registration kind and the dispatch kind are
/// the same by construction: the returned closure is only ever called for `E::KIND`.
///
/// # Which scope the handler runs in
///
/// The one it was written in. An event arrives from the platform, not from the reactive graph, so
/// without this a handler would run with no owner at all — and everything a scope is for would be
/// missing from the one place a component does most of its work. A context looked up there would
/// answer *nothing*, however many scopes above provide it; a signal created there would belong to
/// no owner and be dropped at once. Both fail silently, and the first is how a button written
/// inside a form comes to be a button that cannot find the form.
///
/// The scope is the one that was current where the `on:` binding was written, which is the
/// component's own, and it is captured once when the handler is erased rather than looked up per
/// event.
///
/// ```
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use zgui_geom::{CssPx, Point};
/// use zgui_vocab::{Modifiers, Payload, Phase, PointerEvent, Timestamp};
/// use zgui_view::events::{self, EventType};
/// use zgui_view::{DiscardCommands, DocumentId, EventControl, EventCx, NodeId, erase};
///
/// let clicks = Rc::new(Cell::new(0));
/// let counter = Rc::clone(&clicks);
/// let handler = erase(events::CLICK, move |_ev: &mut EventCx<'_, events::Click>| {
///     counter.set(counter.get() + 1);
/// });
///
/// // What a dispatcher does with it.
/// let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
/// let payload = Payload::Pointer(PointerEvent::mouse(Point::new(CssPx(0.0), CssPx(0.0))));
/// let control = EventControl::new();
/// let mut sink = DiscardCommands;
/// let mut cx = EventCx::new(
///     events::CLICK.kind(), node, node, Phase::Target, Modifiers::NONE, Timestamp::ORIGIN,
///     &payload, &control, &mut sink,
/// );
/// handler(&mut cx);
///
/// assert_eq!(clicks.get(), 1);
/// ```
pub fn erase<E: EventType>(
    _event: E,
    handler: impl Fn(&mut EventCx<'_, E>) + 'static,
) -> Rc<dyn Fn(&mut EventCx<'_>)> {
    let scope = Owner::current();
    Rc::new(move |cx: &mut EventCx<'_>| {
        debug_assert_eq!(
            cx.kind,
            E::KIND,
            "a listener registered for {} was dispatched a {}",
            E::KIND.name(),
            cx.kind.name()
        );
        let mut typed = cx.retype::<E>();
        match &scope {
            Some(scope) => scope.with(|| handler(&mut typed)),
            None => handler(&mut typed),
        }
    })
}

/// One live listener registration.
///
/// A view stores one per `on:` binding it made, so that describing the element again replaces the
/// listeners it registered last time instead of adding a second copy of each. Removal is explicit
/// — see [`ListenerRegistration::remove`] — because a registration cannot reach the backend on its
/// own, and because a view that takes its nodes out of the tree takes their listeners with them.
#[must_use = "a registration that is dropped without being removed leaves its listener attached"]
pub struct ListenerRegistration {
    /// The node it is attached to.
    node: NodeId,
    /// Which registration it is.
    id: ListenerId,
    /// Which event it is for.
    kind: EventKind,
}

impl ListenerRegistration {
    /// Registers `handler` for `event` on `node`.
    pub fn new<E: EventType>(
        dom: &dyn Dom,
        node: NodeId,
        event: E,
        options: ListenerOptions,
        handler: impl Fn(&mut EventCx<'_, E>) + 'static,
    ) -> Self {
        Self::erased(dom, node, event.kind(), options, erase(event, handler))
    }

    /// Registers a handler whose argument type has already been forgotten.
    ///
    /// What an element builder uses: it erases the handler when the attribute is written, long
    /// before it has a node to attach it to, so by the time the registration is made the type the
    /// listener was written against is gone.
    pub fn erased(
        dom: &dyn Dom,
        node: NodeId,
        kind: EventKind,
        options: ListenerOptions,
        handler: Rc<dyn Fn(&mut EventCx<'_>)>,
    ) -> Self {
        let id = dom.add_listener(node, kind, options, handler);
        Self { node, id, kind }
    }

    /// Which event this registration is for.
    pub fn kind(&self) -> EventKind {
        self.kind
    }

    /// Which registration this is.
    pub fn id(&self) -> ListenerId {
        self.id
    }

    /// Removes the listener.
    ///
    /// Explicit rather than on drop, because a registration cannot reach the backend on its own
    /// and a view that removes its nodes removes their listeners with them.
    pub fn remove(self, dom: &dyn Dom) {
        dom.remove_listener(self.node, self.id);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use zgui_geom::{CssPx, Point};
    use zgui_interned::ElementName;
    use zgui_vocab::{ListenerOptions, Modifiers, Payload, Phase, PointerEvent, Timestamp};

    use super::{ListenerRegistration, erase};
    use crate::event::cx::{EventControl, EventCx};
    use crate::event::sink::DiscardCommands;
    use crate::event::view::EventType;
    use crate::events;
    use crate::stub::StubDom;
    use crate::{DocumentId, Dom, NodeId};

    #[test]
    fn a_registration_is_removed_from_the_node_it_was_added_to() {
        let dom = StubDom::new(DocumentId::FIRST);
        let node = dom.create_element(ElementName::new("box"));

        let registration = ListenerRegistration::new(
            &dom,
            node,
            events::CLICK,
            ListenerOptions::DEFAULT,
            |_ev| {},
        );
        assert_eq!(dom.listener_count(), 1);

        registration.remove(&dom);
        assert_eq!(dom.listener_count(), 0);
    }

    #[test]
    fn the_erased_handler_sees_the_payload_the_constant_promised() {
        let seen = Rc::new(Cell::new(false));
        let flag = Rc::clone(&seen);
        let handler = erase(
            events::POINTER_DOWN,
            move |ev: &mut EventCx<'_, events::PointerDown>| {
                flag.set(ev.position.x.0 == 0.0);
            },
        );

        let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
        let payload = Payload::Pointer(PointerEvent::mouse(Point::new(CssPx(0.0), CssPx(0.0))));
        let control = EventControl::new();
        let mut sink = DiscardCommands;
        let mut cx = EventCx::new(
            events::POINTER_DOWN.kind(),
            node,
            node,
            Phase::Target,
            Modifiers::NONE,
            Timestamp::ORIGIN,
            &payload,
            &control,
            &mut sink,
        );
        handler(&mut cx);
        assert!(seen.get());
    }

    #[test]
    fn a_handler_can_reach_the_context_of_the_scope_it_was_written_in() {
        // The failure this rules out is silent: with no scope, the lookup answers `None` and the
        // handler does nothing at all. That is how a submit button inside a form comes to be a
        // button that cannot find its form.
        zgui_reactive::install().ok();
        let scope = zgui_reactive::Mounted::new();
        let found = Rc::new(Cell::new(false));

        let handler = scope.with(|| {
            zgui_reactive::provide_local_context(Rc::new(7_u8));
            let flag = Rc::clone(&found);
            erase(
                events::CLICK,
                move |_ev: &mut EventCx<'_, events::Click>| {
                    flag.set(zgui_reactive::use_local_context::<Rc<u8>>().as_deref() == Some(&7));
                },
            )
        });

        let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
        let payload = Payload::Pointer(PointerEvent::mouse(Point::new(CssPx(0.0), CssPx(0.0))));
        let control = EventControl::new();
        let mut sink = DiscardCommands;
        let mut cx = EventCx::new(
            events::CLICK.kind(),
            node,
            node,
            Phase::Target,
            Modifiers::NONE,
            Timestamp::ORIGIN,
            &payload,
            &control,
            &mut sink,
        );
        handler(&mut cx);

        assert!(
            found.get(),
            "the handler ran outside every scope, so nothing above it could be reached"
        );
        scope.unmount();
    }
}
