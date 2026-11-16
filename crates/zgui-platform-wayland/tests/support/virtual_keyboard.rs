//! A keyboard for a session that has none.
//!
//! A headless compositor has a seat with no keyboard, so nothing sets a lock key. This supplies a
//! virtual keyboard over a connection of the test's own. It uploads a keymap and then states the
//! locked modifiers directly, which the compositor passes on to the surface with the keyboard.

#![allow(dead_code)]

use std::io::Write as _;
use std::os::fd::AsFd as _;
use std::time::Duration;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;

/// The locked-modifier mask of caps lock in the keymap below.
pub(crate) const CAPS_LOCK: u32 = 1 << 1;

/// The locked-modifier mask of num lock in the keymap below.
pub(crate) const NUM_LOCK: u32 = 1 << 4;

/// The format code of an xkb keymap in text form.
const XKB_V1: u32 = 1;

/// A keymap the compositor compiles from its own layout data.
const KEYMAP: &str = "xkb_keymap {
    xkb_keycodes { include \"evdev+aliases(qwerty)\" };
    xkb_types { include \"complete\" };
    xkb_compat { include \"complete\" };
    xkb_symbols { include \"pc+us+inet(evdev)\" };
};\0";

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

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for Silent {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardManagerV1,
        _: <ZwpVirtualKeyboardManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for Silent {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardV1,
        _: <ZwpVirtualKeyboardV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// States each locked-modifier mask of `steps` in turn, `between` apart, on a thread of its own.
///
/// Answers with whether a device could be made at all. A compositor without the protocol cannot
/// answer this property, and the program skips it.
pub(crate) fn lock_over(steps: Vec<u32>, between: Duration) -> bool {
    let Ok(conn) = Connection::connect_to_env() else {
        return false;
    };
    let Ok((globals, mut queue)) = registry_queue_init::<Silent>(&conn) else {
        return false;
    };
    let qh = queue.handle();
    let Ok(manager) = globals.bind::<ZwpVirtualKeyboardManagerV1, _, _>(&qh, 1..=1, ()) else {
        return false;
    };
    let Ok(seat) = globals.bind::<WlSeat, _, _>(&qh, 1..=9, ()) else {
        return false;
    };
    let path = std::env::temp_dir().join(format!("zgui-keymap-{}", std::process::id()));
    let Ok(mut file) = std::fs::File::create(&path) else {
        return false;
    };
    if file.write_all(KEYMAP.as_bytes()).is_err() {
        return false;
    }
    let Ok(keymap) = std::fs::File::open(&path) else {
        return false;
    };
    let _ = std::fs::remove_file(&path);

    std::thread::spawn(move || {
        let _conn = conn;
        let keyboard = manager.create_virtual_keyboard(&seat, &qh, ());
        let mut state = Silent;
        keyboard.keymap(XKB_V1, keymap.as_fd(), KEYMAP.len() as u32);
        let _ = queue.roundtrip(&mut state);
        for locked in steps {
            std::thread::sleep(between);
            keyboard.modifiers(0, 0, locked, 0);
            let _ = queue.roundtrip(&mut state);
        }
        loop {
            let _ = queue.roundtrip(&mut state);
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    true
}
