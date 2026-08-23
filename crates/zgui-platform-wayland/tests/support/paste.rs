//! Reading a selection from outside the application that wrote it.
//!
//! A selection read on the connection that wrote it is answered out of what the application
//! already holds, without a byte reaching the compositor — so a property about the round trip has
//! to ask from somewhere else.
//!
//! It cannot ask as an *ordinary* client, either: the compositor offers a selection only to the
//! client that has keyboard focus, so a second connection with no window is never told there is
//! one. It asks the way a clipboard manager does instead, over `wlr-data-control` — the protocol
//! that exists precisely because reading the clipboard without being focused is a real thing to
//! want. What it verifies is the same thing: that what this application put on the clipboard is
//! what the compositor hands to whoever asks for it.

#![allow(dead_code)]

use std::io::Read;
use std::os::fd::AsFd;
use std::time::Duration;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_device_v1::{
    self, ZwlrDataControlDeviceV1,
};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_offer_v1::{
    self, ZwlrDataControlOfferV1,
};

/// What the compositor said is on the clipboard.
#[derive(Default)]
struct Watching {
    /// The offer it named as the selection.
    offer: Option<ZwlrDataControlOfferV1>,
    /// The media types on it.
    types: Vec<String>,
}

impl Dispatch<WlRegistry, GlobalListContents> for Watching {
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

impl Dispatch<WlSeat, ()> for Watching {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for Watching {
    fn event(
        _: &mut Self,
        _: &ZwlrDataControlManagerV1,
        _: <ZwlrDataControlManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for Watching {
    fn event(
        state: &mut Self,
        _: &ZwlrDataControlOfferV1,
        event: zwlr_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event {
            state.types.push(mime_type);
        }
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for Watching {
    event_created_child!(Watching, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);

    fn event(
        state: &mut Self,
        _: &ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::DataOffer { .. } => state.types.clear(),
            zwlr_data_control_device_v1::Event::Selection { id } => state.offer = id,
            _ => {}
        }
    }
}

/// Asks the compositor for the current selection, the way a clipboard manager does.
///
/// Answers with the text when there is a selection carrying any, and with nothing when there is
/// not one yet — which is the ordinary state for the first moment after a window opens.
pub(crate) fn from_a_second_connection() -> Option<String> {
    let conn = Connection::connect_to_env().ok()?;
    let (globals, mut queue) = registry_queue_init::<Watching>(&conn).ok()?;
    let qh = queue.handle();
    let manager: ZwlrDataControlManagerV1 = globals.bind(&qh, 1..=2, ()).ok()?;
    let seat: WlSeat = globals.bind(&qh, 1..=9, ()).ok()?;
    let device = manager.get_data_device(&seat, &qh, ());

    let mut state = Watching::default();
    // Two round trips: the first announces the offer, the second carries the media types on it.
    queue.roundtrip(&mut state).ok()?;
    queue.roundtrip(&mut state).ok()?;

    let offer = state.offer.clone()?;
    let wanted = state
        .types
        .iter()
        .find(|name| name.eq_ignore_ascii_case("text/plain;charset=utf-8"))
        .or_else(|| state.types.iter().find(|name| name.starts_with("text/")))?
        .clone();

    let (reader, writer) = rustix::pipe::pipe().ok()?;
    offer.receive(wanted, writer.as_fd());
    drop(writer);
    queue.flush().ok()?;

    let mut file = std::fs::File::from(reader);
    let mut taken = String::new();
    // The other end is the application under test, which serves the selection from its own loop.
    // A bound rather than a wait: a selection that never arrives is a failed paste, not a hang.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let mut chunk = [0_u8; 4096];
        match file.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => taken.push_str(&String::from_utf8_lossy(&chunk[..read])),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    device.destroy();
    (!taken.is_empty()).then_some(taken)
}
