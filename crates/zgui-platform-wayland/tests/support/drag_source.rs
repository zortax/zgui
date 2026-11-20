//! A drag from another application, the way a file manager starts one.
//!
//! A drag starts only from a press on the source's own window, so this is a whole client: it maps
//! a window, presses on it with a virtual pointer, starts the drag with that press, carries the
//! pointer to the point it is given and lets go there. The source then says what the compositor
//! made of the drop: performed and finished, or cancelled.

#![allow(dead_code)]

use std::io::Write;
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::WlDataOffer;
use wayland_client::protocol::wl_data_source::{self, WlDataSource};
use wayland_client::protocol::wl_pointer::{self, ButtonState, WlPointer};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, event_created_child,
};
use wayland_protocols::xdg::shell::client::xdg_surface::{self, XdgSurface};
use wayland_protocols::xdg::shell::client::xdg_toplevel::{self, XdgToplevel};
use wayland_protocols::xdg::shell::client::xdg_wm_base::{self, XdgWmBase};
use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1;
use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1;

/// The kernel's code for the primary mouse button.
const BTN_LEFT: u32 = 0x110;

/// The scale pointer positions are given in: thousandths of the output on each axis.
const EXTENT: u32 = 1000;

/// What the source was told about its drag.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Outcome {
    /// Whether the content was asked for.
    pub(crate) sent: bool,
    /// Whether the compositor performed the drop.
    pub(crate) performed: bool,
    /// Whether the destination finished the drop.
    pub(crate) finished: bool,
    /// Whether the compositor cancelled the drag before a drop.
    pub(crate) cancelled: bool,
    /// The action the compositor chose last.
    pub(crate) action: Option<DndAction>,
    /// Whether the compositor chose an action other than none at any time.
    pub(crate) claimed: bool,
}

/// The client's state, as the compositor's events leave it.
struct Source {
    /// The file list the drag carries.
    payload: &'static str,
    /// The configure to acknowledge, with the size it asks for.
    configure: Option<(u32, (i32, i32))>,
    /// The size the toplevel was given.
    size: (i32, i32),
    /// Whether the seat has a pointer.
    pointer: bool,
    /// Whether the pointer is over this window.
    over: bool,
    /// The serial of the press on this window.
    press: Option<u32>,
    /// What the source was told.
    outcome: Outcome,
}

/// Carries a drag of `payload` as `mime` from a window of its own to `to`, in thousandths of the
/// output.
///
/// Blocks until the compositor ended the drag or the patience ran out. Answers with the reason
/// when this session cannot start a drag at all. The caller skips the property then.
pub(crate) fn drag(to: (u32, u32), mime: &str, payload: &'static str) -> Result<Outcome, String> {
    let conn = Connection::connect_to_env().map_err(|error| error.to_string())?;
    let (globals, mut queue) =
        registry_queue_init::<Source>(&conn).map_err(|error| error.to_string())?;
    let qh = queue.handle();
    let compositor: WlCompositor = globals
        .bind(&qh, 1..=6, ())
        .map_err(bind_error("compositor"))?;
    let shm: WlShm = globals.bind(&qh, 1..=1, ()).map_err(bind_error("shm"))?;
    let shell: XdgWmBase = globals
        .bind(&qh, 1..=6, ())
        .map_err(bind_error("xdg shell"))?;
    let seat: WlSeat = globals.bind(&qh, 1..=9, ()).map_err(bind_error("seat"))?;
    let devices: WlDataDeviceManager = globals
        .bind(&qh, 3..=3, ())
        .map_err(bind_error("data device manager"))?;
    let hands: ZwlrVirtualPointerManagerV1 = globals
        .bind(&qh, 1..=2, ())
        .map_err(bind_error("virtual pointer manager"))?;

    let mut state = Source {
        payload,
        configure: None,
        size: (0, 0),
        pointer: false,
        over: false,
        press: None,
        outcome: Outcome::default(),
    };
    let hand = hands.create_virtual_pointer(Some(&seat), &qh, ());
    let mut clock = 0;
    let mut step = |queue: &mut EventQueue<Source>,
                    state: &mut Source,
                    run: &dyn Fn(&ZwlrVirtualPointerV1, u32)| {
        clock += 16;
        run(&hand, clock);
        hand.frame();
        let _ = queue.roundtrip(state);
        std::thread::sleep(Duration::from_millis(60));
        let _ = queue.roundtrip(state);
    };
    let at = |point: (u32, u32)| {
        move |hand: &ZwlrVirtualPointerV1, time: u32| {
            hand.motion_absolute(time, point.0, point.1, EXTENT, EXTENT);
        }
    };

    // The compositor attaches the device when it first does something. A pointer opened on a seat
    // that has none yet stays inert, so it is opened after the capability arrives.
    step(&mut queue, &mut state, &at((500, 500)));
    if !state.pointer {
        return Err("the seat has no pointer, even with a virtual one attached".to_owned());
    }
    let _pointer = seat.get_pointer(&qh, ());
    let device = devices.get_data_device(&seat, &qh, ());

    let surface = compositor.create_surface(&qh, ());
    let shell_surface = shell.get_xdg_surface(&surface, &qh, ());
    let toplevel = shell_surface.get_toplevel(&qh, ());
    toplevel.set_title("drag source".to_owned());
    toplevel.set_app_id("dev.zgui.platform-wayland-test.drag-source".to_owned());
    surface.commit();
    // The first configure can leave the size to the client, and the tiled size follows in a later
    // one. Every configure in the first moment is answered with a buffer of the size it names.
    let mut buffer = None;
    let deadline = Instant::now() + Duration::from_millis(800);
    while Instant::now() < deadline {
        let _ = queue.roundtrip(&mut state);
        if let Some((serial, size)) = state.configure.take() {
            shell_surface.ack_configure(serial);
            let size = (
                if size.0 > 0 { size.0 } else { 200 },
                if size.1 > 0 { size.1 } else { 200 },
            );
            if let Some(old) = buffer.replace(paint(&shm, &qh, &surface, size)?) {
                WlBuffer::destroy(&old);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if buffer.is_none() {
        return Err("the source window was never configured".to_owned());
    }

    // The window is tiled beside the destination, and which half it took is the compositor's
    // choice. The pointer finds out.
    let mut origin = None;
    for candidate in [(750, 500), (250, 500)] {
        step(&mut queue, &mut state, &at(candidate));
        if state.over {
            origin = Some(candidate);
            break;
        }
    }
    let Some(origin) = origin else {
        return Err("the pointer never reached the source window".to_owned());
    };

    step(&mut queue, &mut state, &|hand, time| {
        hand.button(time, BTN_LEFT, ButtonState::Pressed);
    });
    let Some(press) = state.press else {
        return Err("the press on the source window never arrived".to_owned());
    };
    let source = devices.create_data_source(&qh, ());
    source.offer(mime.to_owned());
    source.set_actions(DndAction::Copy | DndAction::Move);
    device.start_drag(Some(&source), &surface, None, press);
    let _ = queue.roundtrip(&mut state);

    // Carried in steps, as a hand does, so that the destination sees the drag enter and move.
    let target = if origin.0 > 500 {
        to
    } else {
        (EXTENT - to.0, to.1)
    };
    for index in 1..=8_i64 {
        let along = |from: u32, to: u32| {
            u32::try_from(i64::from(from) + (i64::from(to) - i64::from(from)) * index / 8)
                .unwrap_or(0)
        };
        step(
            &mut queue,
            &mut state,
            &at((along(origin.0, target.0), along(origin.1, target.1))),
        );
    }
    // The destination answers the enter from its own loop. It is given time to.
    for _ in 0..5 {
        step(&mut queue, &mut state, &at(target));
    }
    step(&mut queue, &mut state, &|hand, time| {
        hand.button(time, BTN_LEFT, ButtonState::Released);
    });

    let deadline = Instant::now() + Duration::from_secs(3);
    while !(state.outcome.finished || state.outcome.cancelled) && Instant::now() < deadline {
        let _ = queue.roundtrip(&mut state);
        std::thread::sleep(Duration::from_millis(30));
    }
    source.destroy();
    device.release();
    toplevel.destroy();
    shell_surface.destroy();
    surface.destroy();
    let _ = queue.roundtrip(&mut state);
    Ok(state.outcome)
}

/// The reason a global the drag needs is missing.
fn bind_error(name: &'static str) -> impl Fn(wayland_client::globals::BindError) -> String {
    move |error| format!("no {name}: {error}")
}

/// Gives the window one plain buffer of `size`, so that it is mapped and the pointer can hit it.
fn paint(
    shm: &WlShm,
    qh: &QueueHandle<Source>,
    surface: &WlSurface,
    size: (i32, i32),
) -> Result<WlBuffer, String> {
    let stride = size.0 * 4;
    let bytes = stride * size.1;
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let name = directory.join(format!("zgui-drag-source-{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&name)
        .map_err(|error| format!("no buffer file: {error}"))?;
    let _ = std::fs::remove_file(&name);
    let pixels = vec![0x80_u8; usize::try_from(bytes).unwrap_or(0)];
    file.write_all(&pixels)
        .map_err(|error| format!("the buffer file would not fill: {error}"))?;
    let pool = shm.create_pool(file.as_fd(), bytes, qh, ());
    let buffer = pool.create_buffer(0, size.0, size.1, stride, wl_shm::Format::Xrgb8888, qh, ());
    pool.destroy();
    surface.attach(Some(&buffer), 0, 0);
    surface.damage_buffer(0, 0, size.0, size.1);
    surface.commit();
    Ok(buffer)
}

impl Dispatch<WlRegistry, GlobalListContents> for Source {
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

impl Dispatch<WlSeat, ()> for Source {
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
            state.pointer = capabilities.contains(wl_seat::Capability::Pointer);
        }
    }
}

impl Dispatch<WlPointer, ()> for Source {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { .. } => state.over = true,
            wl_pointer::Event::Leave { .. } => state.over = false,
            wl_pointer::Event::Button {
                serial,
                state: WEnum::Value(ButtonState::Pressed),
                ..
            } => state.press = Some(serial),
            _ => {}
        }
    }
}

impl Dispatch<XdgWmBase, ()> for Source {
    fn event(
        _: &mut Self,
        shell: &XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            shell.pong(serial);
        }
    }
}

impl Dispatch<XdgSurface, ()> for Source {
    fn event(
        state: &mut Self,
        _: &XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            let size = state.size;
            state.configure = Some((serial, size));
        }
    }
}

impl Dispatch<XdgToplevel, ()> for Source {
    fn event(
        state: &mut Self,
        _: &XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Configure { width, height, .. } = event {
            state.size = (width, height);
        }
    }
}

impl Dispatch<WlDataDevice, ()> for Source {
    event_created_child!(Source, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);

    fn event(
        _: &mut Self,
        _: &WlDataDevice,
        _: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The drag also passes over the source's own window. Nothing here takes it.
    }
}

impl Dispatch<WlDataSource, ()> for Source {
    fn event(
        state: &mut Self,
        _: &WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { fd, .. } => {
                let mut pipe = std::fs::File::from(fd);
                let _ = pipe.write_all(state.payload.as_bytes());
                state.outcome.sent = true;
            }
            wl_data_source::Event::DndDropPerformed => state.outcome.performed = true,
            wl_data_source::Event::DndFinished => state.outcome.finished = true,
            // A compositor can also cancel a source after its drop finished, when the offer is
            // destroyed. Only a cancel before the drop ends the drag.
            wl_data_source::Event::Cancelled => {
                state.outcome.cancelled |= !state.outcome.performed;
            }
            wl_data_source::Event::Action {
                dnd_action: WEnum::Value(action),
            } => {
                state.outcome.action = Some(action);
                state.outcome.claimed |= !action.is_empty();
            }
            _ => {}
        }
    }
}

/// Objects whose events the source has no use for.
macro_rules! ignored {
    ($($object:ty),* $(,)?) => {$(
        impl Dispatch<$object, ()> for Source {
            fn event(
                _: &mut Self,
                _: &$object,
                _: <$object as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    )*};
}

ignored!(
    WlCompositor,
    WlShm,
    WlShmPool,
    WlBuffer,
    WlSurface,
    WlDataDeviceManager,
    WlDataOffer,
    ZwlrVirtualPointerManagerV1,
    ZwlrVirtualPointerV1,
);
