//! What the compositor says its seat has.
//!
//! A property about input that finds none has two very different reasons to: the session has no
//! device to produce any, or this backend never opened the one it was offered. The second is a
//! defect that hides as a skip — an application nothing can be typed into, reported as a machine
//! that had nothing to say.
//!
//! So the compositor is asked directly, over a connection of the test's own. What it advertises
//! there it advertises to everyone, so a capability seen here and no events under it is this
//! backend's fault and is failed rather than skipped.

#![allow(dead_code)]

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

/// What a seat was advertised as having.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Has {
    /// Whether it can type.
    pub(crate) keyboard: bool,
    /// Whether it can point.
    pub(crate) pointer: bool,
    /// Whether it can be touched.
    pub(crate) touch: bool,
}

impl Dispatch<WlRegistry, GlobalListContents> for Has {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for Has {
    fn event(
        state: &mut Self,
        _: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        {
            state.keyboard = capabilities.contains(wl_seat::Capability::Keyboard);
            state.pointer = capabilities.contains(wl_seat::Capability::Pointer);
            state.touch = capabilities.contains(wl_seat::Capability::Touch);
        }
    }
}

/// Asks the compositor what is attached to its seat right now.
///
/// Answers with nothing when there is no compositor or no seat, which is the same thing every
/// caller does with a seat that has no devices.
pub(crate) fn advertised() -> Has {
    let Ok(conn) = Connection::connect_to_env() else {
        return Has::default();
    };
    let Ok((globals, mut queue)) = registry_queue_init::<Has>(&conn) else {
        return Has::default();
    };
    let qh = queue.handle();
    let Ok(_seat) = globals.bind::<WlSeat, _, _>(&qh, 1..=9, ()) else {
        return Has::default();
    };
    let mut has = Has::default();
    // One round trip: the capabilities arrive with the seat, unasked.
    let _ = queue.roundtrip(&mut has);
    has
}
