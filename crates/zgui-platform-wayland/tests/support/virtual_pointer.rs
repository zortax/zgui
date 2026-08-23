//! An input device for a session that has none.
//!
//! A headless compositor has a seat with no devices attached to it, so it advertises no pointer
//! capability and there is nothing for a property about input to move. This supplies one: a
//! virtual pointer, created over a connection of the test's own, which the compositor treats as
//! real hardware — the seat gains its pointer capability, the backend under test opens a
//! `wl_pointer` for it exactly as it would for a mouse, and what comes back is what a mouse would
//! have produced.

#![allow(dead_code)]

use std::time::Duration;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1;
use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1;

/// The kernel's code for the primary mouse button.
const BTN_LEFT: u32 = 0x110;

/// Whether a button went down or came up.
type ButtonState = wayland_client::protocol::wl_pointer::ButtonState;

/// The dispatch target, which has nothing to receive: every object here is write-only.
struct Silent;

impl Dispatch<WlRegistry, GlobalListContents> for Silent {
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

impl Dispatch<WlSeat, ()> for Silent {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: <WlSeat as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrVirtualPointerManagerV1, ()> for Silent {
    fn event(
        _: &mut Self,
        _: &ZwlrVirtualPointerManagerV1,
        _: <ZwlrVirtualPointerManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrVirtualPointerV1, ()> for Silent {
    fn event(
        _: &mut Self,
        _: &ZwlrVirtualPointerV1,
        _: <ZwlrVirtualPointerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// Moves a virtual pointer over the whole output and clicks, on a thread of its own.
///
/// On its own thread because the loop under test owns this one, and every step here waits for the
/// compositor to answer. Each step is separated by a pause so that the events arrive as a stream
/// rather than as one batch — which is what a person's hand produces and what the backend has to
/// cope with.
///
/// Answers with whether a device could be made at all. A compositor without the protocol is a
/// compositor this property cannot be asked about, which is a skip rather than a failure.
pub(crate) fn click_over(extent: (u32, u32), at: (u32, u32)) -> bool {
    let Ok(conn) = Connection::connect_to_env() else {
        return false;
    };
    let Ok((globals, mut queue)) = registry_queue_init::<Silent>(&conn) else {
        return false;
    };
    let qh = queue.handle();
    let manager = match globals.bind::<ZwlrVirtualPointerManagerV1, _, _>(&qh, 1..=2, ()) {
        Ok(manager) => manager,
        Err(error) => {
            eprintln!("no virtual pointer manager: {error}");
            return false;
        }
    };
    // The seat is named rather than left to the compositor: a virtual pointer with no seat is
    // attached to whichever the compositor prefers, and a session with more than one has no
    // preference worth relying on.
    let seat = globals.bind::<WlSeat, _, _>(&qh, 1..=9, ()).ok();

    std::thread::spawn(move || {
        // The connection is moved in rather than left behind: dropping it would take the virtual
        // device down with it, and the device has to outlive every event it is meant to produce.
        let _conn = conn;
        let pointer = manager.create_virtual_pointer(seat.as_ref(), &qh, ());
        let mut state = Silent;
        let mut clock = 0;
        let mut step = |run: &mut dyn FnMut(&ZwlrVirtualPointerV1, u32)| {
            clock += 16;
            run(&pointer, clock);
            pointer.frame();
            let _ = queue.roundtrip(&mut state);
            std::thread::sleep(Duration::from_millis(60));
        };

        // The compositor only attaches the device once it has seen it do something, so the first
        // motion is what makes the seat gain its pointer capability.
        step(&mut |pointer, time| {
            pointer.motion_absolute(time, at.0, at.1, extent.0, extent.1);
        });
        step(&mut |pointer, time| {
            pointer.motion_absolute(time, at.0 + 4, at.1 + 4, extent.0, extent.1);
        });
        step(&mut |pointer, time| {
            pointer.button(time, BTN_LEFT, ButtonState::Pressed);
        });
        step(&mut |pointer, time| {
            pointer.button(time, BTN_LEFT, ButtonState::Released);
        });
        // Never destroyed. Destroying it takes the seat's pointer capability with it, and the
        // capability is what says whether a property that saw no pointer events saw none because
        // there was no device or because this backend opened nothing. It is released when the
        // process ends, which is the moment after the property has been answered.
        loop {
            let _ = queue.roundtrip(&mut state);
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    true
}
