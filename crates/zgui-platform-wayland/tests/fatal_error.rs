//! A protocol error ends the run with that error.
//!
//! The compositor refuses a request on the application's connection and closes it. The run
//! returns the refusal, with the object it was made on.

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
#[path = "support/mod.rs"]
mod support;

/// Runs the test where the platform under test exists, and does nothing elsewhere.
fn main() {
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    supported::main();
}

/// The test, built only where the platform under test exists.
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
mod supported {
    use super::support;

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
        let (globals, queue) =
            registry_queue_init::<Probe>(conn).expect("the probe lists the globals");
        let handle = queue.handle();
        let compositor: WlCompositor = globals
            .bind(&handle, 3..=6, ())
            .expect("the compositor offers a surface with a buffer scale");
        let surface = compositor.create_surface(&handle, ());
        surface.set_buffer_scale(0);
        let _ = conn.flush();
    }

    pub(super) fn main() {
        support::fatal::assert_run_ends(
            "a protocol error ends the run with that error",
            refuse,
            "refused a request on wl_surface",
        );
    }
}
