//! An ordinary window, on the shell this backend binds itself.

use smithay_client_toolkit::reexports::client::protocol::wl_seat::WlSeat;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::{
    self, Mode, ZxdgToplevelDecorationV1,
};
use wayland_protocols::xdg::shell::client::xdg_surface::{self, XdgSurface};
use wayland_protocols::xdg::shell::client::xdg_toplevel::{self, ResizeEdge, XdgToplevel};
use zgui_geom::{Css, CssPx, Size};
use zgui_platform::{DecorationSource, Decorations, SurfaceId};

use crate::driver::WaylandState;
use crate::surface::role::xdg::XdgShell;
use crate::surface::role::xdg::configure::Pending;

/// A window: the surface, the shell object over it, and the frame the compositor draws.
#[derive(Debug)]
pub struct Toplevel {
    /// The surface underneath.
    wl: WlSurface,
    /// The shell object over it, which is what a configure arrives on.
    xdg: XdgSurface,
    /// The window role.
    toplevel: XdgToplevel,
    /// The frame the compositor draws, where it is willing to.
    decoration: Option<ZxdgToplevelDecorationV1>,
}

impl Toplevel {
    /// Makes a window out of `wl`, numbered `id`.
    ///
    /// The order is the protocol's: the shell object, then the role, then everything about it, and
    /// then a commit with no buffer — which is what asks the compositor to configure it. Attaching
    /// anything before that answer is a protocol error.
    pub fn new(
        shell: &XdgShell,
        qh: &QueueHandle<WaylandState>,
        wl: WlSurface,
        id: SurfaceId,
        decorations: Decorations,
    ) -> Self {
        let xdg = shell.base().get_xdg_surface(&wl, qh, id);
        let toplevel = xdg.get_toplevel(qh, id);
        let decoration = shell.decorations().map(|manager| {
            let decoration = manager.get_toplevel_decoration(&toplevel, qh, id);
            decoration.set_mode(mode(decorations));
            decoration
        });
        Self {
            wl,
            xdg,
            toplevel,
            decoration,
        }
    }

    /// The surface underneath.
    pub const fn wl_surface(&self) -> &WlSurface {
        &self.wl
    }

    /// The shell object a configure arrives on.
    pub const fn xdg_surface(&self) -> &XdgSurface {
        &self.xdg
    }

    /// The window role.
    pub const fn xdg_toplevel(&self) -> &XdgToplevel {
        &self.toplevel
    }

    /// Names the window in whatever the desktop shows windows in.
    pub fn set_title(&self, title: &str) {
        self.toplevel.set_title(title.to_owned());
    }

    /// Names the application the desktop groups this window under.
    pub fn set_app_id(&self, id: &str) {
        self.toplevel.set_app_id(id.to_owned());
    }

    /// Sets the smallest and largest extents the user may drag to.
    pub fn set_bounds(&self, min: Option<Size<CssPx, Css>>, max: Option<Size<CssPx, Css>>) {
        let (width, height) = whole(min);
        self.toplevel.set_min_size(width, height);
        let (width, height) = whole(max);
        self.toplevel.set_max_size(width, height);
    }

    /// Says which part of the surface is the window, as opposed to its shadow.
    ///
    /// An extent of zero on either axis is a protocol error rather than a small window, so both are
    /// clamped. The origin stays at the corner because this backend draws no shadow outside the
    /// window it was given.
    pub fn set_geometry(&self, size: Size<CssPx, Css>) {
        self.xdg.set_window_geometry(
            0,
            0,
            (size.width.0.round() as i32).max(1),
            (size.height.0.round() as i32).max(1),
        );
    }

    /// Acknowledges a configure, which must happen before the frame that answers it is committed.
    pub fn ack(&self, serial: u32) {
        self.xdg.ack_configure(serial);
    }

    /// Asks for the window to be maximised, or restored.
    pub fn set_maximized(&self, maximized: bool) {
        if maximized {
            self.toplevel.set_maximized();
        } else {
            self.toplevel.unset_maximized();
        }
    }

    /// Asks for the window to fill the screen, or to stop.
    pub fn set_fullscreen(&self, fullscreen: bool) {
        if fullscreen {
            // No output is named: a compositor never changes an output's mode for a client, so the
            // two kinds of full screen are one request and the choice of screen is the desktop's.
            self.toplevel.set_fullscreen(None);
        } else {
            self.toplevel.unset_fullscreen();
        }
    }

    /// Asks for the window to be put away.
    ///
    /// There is no way back: a compositor tells a client nothing about being minimised and offers
    /// no request to undo it. Restoring is the person's, through the desktop.
    pub fn set_minimized(&self) {
        self.toplevel.set_minimized();
    }

    /// Asks for a frame, or for none.
    pub fn request_decorations(&self, decorations: Decorations) {
        if let Some(decoration) = &self.decoration {
            decoration.set_mode(mode(decorations));
        }
    }

    /// Hands a move of the window over to the compositor.
    pub fn begin_move(&self, seat: &WlSeat, serial: u32) {
        self.toplevel._move(seat, serial);
    }

    /// Hands a resize of the window over to the compositor.
    pub fn begin_resize(&self, seat: &WlSeat, serial: u32, edge: ResizeEdge) {
        self.toplevel.resize(seat, serial, edge);
    }
}

impl Drop for Toplevel {
    /// Takes the objects down innermost first.
    ///
    /// Order is not a preference: a role object outliving the shell object it was made from is a
    /// protocol error rather than a leak, and so is a decoration outliving its role.
    fn drop(&mut self) {
        if let Some(decoration) = &self.decoration {
            decoration.destroy();
        }
        self.toplevel.destroy();
        self.xdg.destroy();
        self.wl.destroy();
    }
}

/// The frame mode a decoration preference asks for.
///
/// Asking the compositor to draw it is the default and is what a desktop expects: a window whose
/// frame matches every other window's. Everything else means the application draws its own
/// furniture, so the compositor is told to draw none — a compositor that drew one anyway would
/// leave two title bars stacked.
pub const fn mode(decorations: Decorations) -> Mode {
    match decorations {
        Decorations::Full => Mode::ServerSide,
        _ => Mode::ClientSide,
    }
}

/// Who ended up drawing the frame.
pub const fn source(mode: Mode) -> DecorationSource {
    match mode {
        Mode::ServerSide => DecorationSource::Platform,
        _ => DecorationSource::Application,
    }
}

/// An extent as the shell's requests take it: whole pixels, and zero meaning "no limit".
fn whole(size: Option<Size<CssPx, Css>>) -> (i32, i32) {
    size.map_or((0, 0), |size| {
        (
            size.width.0.round().max(0.0) as i32,
            size.height.0.round().max(0.0) as i32,
        )
    })
}

impl Dispatch<XdgSurface, SurfaceId> for WaylandState {
    fn event(
        state: &mut Self,
        _xdg: &XdgSurface,
        event: <XdgSurface as Proxy>::Event,
        id: &SurfaceId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // The one event that says a description is complete, and the only place it is acted on.
        if let xdg_surface::Event::Configure { serial } = event {
            state.configure_surface(*id, serial);
        }
    }
}

impl Dispatch<XdgToplevel, SurfaceId> for WaylandState {
    fn event(
        state: &mut Self,
        _toplevel: &XdgToplevel,
        event: <XdgToplevel as Proxy>::Event,
        id: &SurfaceId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            // Stashed rather than applied. What the compositor is describing is not complete until
            // the surface says so, and acting on half of it resizes to an extent whose state has
            // not arrived.
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
            } => {
                let pending = Pending::read((width, height), &states);
                tracing::trace!(
                    ?pending,
                    states = ?states
                        .chunks_exact(4)
                        .filter_map(|chunk| <[u8; 4]>::try_from(chunk).ok())
                        .map(u32::from_ne_bytes)
                        .collect::<Vec<_>>(),
                    "the compositor described a window"
                );
                state.stash_configure(*id, pending);
            }
            xdg_toplevel::Event::Close => {
                state.report(*id, zgui_platform::SurfaceEvent::CloseRequested);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZxdgToplevelDecorationV1, SurfaceId> for WaylandState {
    fn event(
        state: &mut Self,
        _decoration: &ZxdgToplevelDecorationV1,
        event: <ZxdgToplevelDecorationV1 as Proxy>::Event,
        id: &SurfaceId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // The compositor's answer, which is what actually decides who draws the frame: a request
        // for a server-drawn one may be refused, and an application that assumed it was granted
        // shows a window with no title bar at all.
        if let zxdg_toplevel_decoration_v1::Event::Configure { mode } = event
            && let Ok(mode) = mode.into_result()
        {
            state.decorated_by(*id, source(mode));
        }
    }
}
