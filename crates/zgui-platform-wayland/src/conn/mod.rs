//! The connection to the compositor, and everything bound on it.

pub mod globals;

pub use crate::conn::globals::Extras;

use smithay_client_toolkit::reexports::client::globals::{GlobalList, registry_queue_init};
use smithay_client_toolkit::reexports::client::{Connection, EventQueue};
use zgui_platform::PlatformError;

use crate::driver::WaylandState;

/// A connection to the compositor, with its globals enumerated.
///
/// The registry round trip happens here and once: everything bound afterwards is bound against
/// this list rather than against a second trip, and a compositor that is missing something says so
/// at start-up rather than at the moment a menu is first opened.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when there is no compositor to connect to, which is how the
/// caller learns to fall back to another backend rather than failing to start.
pub fn open() -> Result<(Connection, GlobalList, EventQueue<WaylandState>), PlatformError> {
    let conn = Connection::connect_to_env()
        .map_err(|error| PlatformError::Backend(format!("no wayland display: {error}")))?;
    let (globals, queue) = registry_queue_init(&conn)
        .map_err(|error| PlatformError::Backend(format!("the wayland registry: {error}")))?;
    Ok((conn, globals, queue))
}

/// Whether this process has a Wayland display to connect to at all.
///
/// A cheap answer that costs no socket: it is what selects this backend over the portable one, and
/// the selection happens before either has been started.
pub fn is_available() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty())
        || std::env::var_os("WAYLAND_SOCKET").is_some_and(|socket| !socket.is_empty())
}
