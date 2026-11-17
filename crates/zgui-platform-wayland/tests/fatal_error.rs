//! A protocol error ends the run with that error.
//!
//! The compositor refuses a request on the application's connection and closes it. The run
//! returns the refusal, with the object it was made on.

#[path = "support/mod.rs"]
mod support;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

/// The dispatch target of the refused request, which receives nothing it acts on.
struct Probe;

impl Dispatch<WlRegistry, GlobalListContents> for Probe {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlCompositor, ()> for Probe {
    fn event(
        _: &mut Self,
        _: &WlCompositor,
        _: <WlCompositor as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSurface, ()> for Probe {
    fn event(
        _: &mut Self,
        _: &WlSurface,
        _: <WlSurface as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// Sends a request the compositor refuses: a buffer scale of zero on a surface of its own.
fn refuse(conn: &Connection) {
    let (globals, queue) = registry_queue_init::<Probe>(conn).expect("the probe lists the globals");
    let handle = queue.handle();
    let compositor: WlCompositor = globals
        .bind(&handle, 3..=6, ())
        .expect("the compositor offers a surface with a buffer scale");
    let surface = compositor.create_surface(&handle, ());
    surface.set_buffer_scale(0);
    let _ = conn.flush();
}

fn main() {
    support::fatal::assert_run_ends(
        "a protocol error ends the run with that error",
        refuse,
        "refused a request on wl_surface",
    );
}
