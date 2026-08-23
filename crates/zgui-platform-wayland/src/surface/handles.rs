//! The compositor's own pointers, as a graphics API takes them.

use std::ptr::NonNull;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use smithay_client_toolkit::reexports::client::{Connection, Proxy};
use wayland_client::protocol::wl_surface::WlSurface;

use crate::surface::WaylandSurface;

/// The connection's display, as a graphics API names it.
fn display(conn: &Connection) -> Result<RawDisplayHandle, HandleError> {
    let display =
        NonNull::new(conn.backend().display_ptr().cast()).ok_or(HandleError::Unavailable)?;
    Ok(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
        display,
    )))
}

/// The surface, as a graphics API names it.
fn window(surface: &WlSurface) -> Result<RawWindowHandle, HandleError> {
    let surface = NonNull::new(surface.id().as_ptr().cast()).ok_or(HandleError::Unavailable)?;
    Ok(RawWindowHandle::Wayland(WaylandWindowHandle::new(surface)))
}

impl HasDisplayHandle for WaylandSurface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = display(self.connection())?;
        // SAFETY: the handle borrows the connection this surface holds, and the borrow it is given
        // is the borrow of `self`. The connection is an owned handle on this surface and is
        // therefore alive for exactly as long as the returned handle may be used.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

impl HasWindowHandle for WaylandSurface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = window(self.wl_surface())?;
        // SAFETY: the pointer is this surface's own `wl_surface`, which this surface owns and
        // destroys only when it is itself dropped. The handle borrows `self`, so it cannot outlive
        // the object it names.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}
