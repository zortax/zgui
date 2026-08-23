//! What a surface is to the compositor, and what each kind can be asked.

pub mod layer;
pub mod popup;
pub mod toplevel;
pub mod xdg;

use smithay_client_toolkit::shell::WaylandSurface as _;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use wayland_client::protocol::wl_surface::WlSurface;

pub use crate::surface::role::xdg::popup::Popup;
pub use crate::surface::role::xdg::toplevel::Toplevel;

/// The shell object this surface is.
///
/// Every arm owns the same `wl_surface` underneath and differs only in what the compositor will
/// let it ask for. A request that belongs to another arm answers with
/// [`Unsupported`](zgui_platform::Unsupported) rather than being silently ignored, because a menu
/// that quietly became a second application window is worse than a menu that did not open.
#[derive(Debug)]
pub enum Role {
    /// An ordinary window.
    Toplevel(Box<Toplevel>),
    /// A surface placed against a rectangle of another, dismissed with it.
    Popup(Box<Popup>),
    /// A part of the desktop shell: a panel, a wallpaper, a lock screen.
    Layer(Box<LayerSurface>),
}

impl Role {
    /// The surface underneath, which every arm has.
    pub fn wl_surface(&self) -> &WlSurface {
        match self {
            Self::Toplevel(window) => window.wl_surface(),
            Self::Popup(popup) => popup.wl_surface(),
            Self::Layer(layer) => layer.wl_surface(),
        }
    }

    /// The window, when this is one.
    pub fn window(&self) -> Option<&Toplevel> {
        match self {
            Self::Toplevel(window) => Some(window),
            _ => None,
        }
    }

    /// The window, mutably, for the one thing a pop-up remembers about itself.
    pub fn popup_mut(&mut self) -> Option<&mut Popup> {
        match self {
            Self::Popup(popup) => Some(popup),
            _ => None,
        }
    }

    /// The shell object a configure arrives on, for the two roles that have one.
    pub fn xdg_surface(
        &self,
    ) -> Option<&wayland_protocols::xdg::shell::client::xdg_surface::XdgSurface> {
        match self {
            Self::Toplevel(window) => Some(window.xdg_surface()),
            Self::Popup(popup) => Some(popup.xdg_surface()),
            Self::Layer(_) => None,
        }
    }

    /// Acknowledges a configure on whichever role received it.
    pub fn ack(&self, serial: u32) {
        match self {
            Self::Toplevel(window) => window.ack(serial),
            Self::Popup(popup) => popup.ack(serial),
            // The toolkit's layer surface acknowledges its own.
            Self::Layer(_) => {}
        }
    }

    /// The layer surface, when this is one.
    pub fn layer(&self) -> Option<&LayerSurface> {
        match self {
            Self::Layer(layer) => Some(layer),
            _ => None,
        }
    }

    /// The pop-up, when this is one.
    pub fn popup(&self) -> Option<&Popup> {
        match self {
            Self::Popup(popup) => Some(popup),
            _ => None,
        }
    }

    /// Commits the surface's pending state.
    ///
    /// This is what keeps the frame chain alive on a turn that drew nothing: a frame callback was
    /// asked for before the application was given its redraw, and a callback rides a commit.
    /// Ending such a turn without one stops the compositor answering, for good.
    pub fn commit(&self) {
        self.wl_surface().commit();
    }
}
