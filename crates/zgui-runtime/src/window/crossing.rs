//! Telling elements that the pointer arrived on them, and that it left.
//!
//! The pointer moving from one control to its neighbour is not one fact but two lists: the elements
//! it is now inside and was not, and the elements it was inside and is not. The router already
//! computes exactly those two lists, because `:hover` has to be written up a whole path and moving
//! between two siblings must not rewrite the ancestors they share. What was missing is that the
//! same two lists are what `pointer_enter` and `pointer_leave` mean.
//!
//! Without this, those two events fire only when the pointer enters or leaves the *surface*, which
//! is the compositor's boundary rather than an element's — so an element the pointer walked onto
//! was never told, and every behaviour written as "while the pointer is on this" was dead. A
//! tooltip is the plainest case: it is a delay armed on arrival and disarmed on departure, and with
//! neither ever announced it can only stay shut.
//!
//! # Why each element is told separately
//!
//! [`PointerEnter`](EventKind::PointerEnter) and [`PointerLeave`](EventKind::PointerLeave) do not
//! bubble, and that is the whole reason they are the events a wrapper listens for: a wrapper around
//! a button wants to hear about the pointer being anywhere inside it, and it must not hear about it
//! again from every child the pointer crosses on the way. So the announcement is one dispatch per
//! element that crossed the boundary, not one dispatch that travels a path — a wrapper that only
//! received what bubbled up from the element actually under the pointer would receive nothing at
//! all.
//!
//! # Why they are queued rather than sent where they are computed
//!
//! The crossing is computed while the event that caused it is still being routed, with the
//! document's change batch open. Dispatching from there would begin one dispatch inside another and
//! re-enter that batch. So the crossing is recorded and the events go out where every other
//! consequence of a handler goes out: after the dispatch that caused it has finished.

use zgui_dom::NodeKey;
use zgui_vocab::{EventKind, Payload, PointerEvent, Timestamp};

use crate::window::Window;

/// One element the pointer arrived on or left, and the pointer that did it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Crossing {
    /// The element.
    pub(crate) node: NodeKey,
    /// Which way it crossed: onto the element, or off it.
    pub(crate) kind: EventKind,
    /// Where the pointer was, as a handler reads it off the event.
    pub(crate) pointer: PointerEvent,
}

impl Window {
    /// Records what a pointer's move did to the set of elements it is inside.
    ///
    /// Departures first and then arrivals, so that a handler asking what the pointer is on is
    /// answered about where it is rather than about where it was on the way. Within each, the order
    /// is the one the browser event model uses and the one a nested wrapper needs: a departure is
    /// announced from the innermost element outwards, and an arrival from the outermost inwards.
    pub(crate) fn note_crossings(&mut self, moved: &zgui_input::Moved, pointer: PointerEvent) {
        for node in moved.left.iter().rev() {
            self.pending_crossings.push(Crossing {
                node: *node,
                kind: EventKind::PointerLeave,
                pointer,
            });
        }
        for node in &moved.entered {
            self.pending_crossings.push(Crossing {
                node: *node,
                kind: EventKind::PointerEnter,
                pointer,
            });
        }
    }

    /// The pointer event a crossing that no pointer event caused is announced with.
    ///
    /// Content moving under a cursor that has not moved crosses just as real a boundary as a cursor
    /// moving over stationary content, and a handler reads the same field off either. So the
    /// position is the one the router last saw, in the units a handler is given.
    pub(crate) fn pointer_now(&self) -> PointerEvent {
        let position = self
            .router
            .pointers()
            .all()
            .next()
            .map(|(_, point)| {
                zgui_geom::Point::new(
                    zgui_geom::CssPx(point.x.0 / self.scale),
                    zgui_geom::CssPx(point.y.0 / self.scale),
                )
            })
            .unwrap_or(zgui_geom::Point::new(
                zgui_geom::CssPx(0.0),
                zgui_geom::CssPx(0.0),
            ));
        PointerEvent::mouse(position)
    }

    /// Tells every element that has crossed a boundary since it was last asked.
    ///
    /// Taken rather than drained in place: a handler for one of these opens a surface, and whatever
    /// that surface's arrival puts under the pointer belongs to the next round rather than to this
    /// one.
    pub(crate) fn announce_crossings(&mut self, timestamp: Timestamp) {
        for crossing in core::mem::take(&mut self.pending_crossings) {
            self.dispatch_synthetic(
                crossing.node,
                crossing.kind,
                Payload::Pointer(crossing.pointer),
                timestamp,
            );
        }
    }
}
