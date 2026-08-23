//! The desktop shell, bound at the version that says when a window has stopped being drawn.
//!
//! # Why this is not the toolkit's
//!
//! The toolkit binds `xdg_wm_base` at version five, and the state that says a surface is no longer
//! being repainted arrived in version six. A client bound below it never receives that state
//! however new the compositor is — and the compositors that send it check the client's version
//! before they do, so there is no way to observe it from a five.
//!
//! That state is the only signal on this desktop that reliably says a window is not being seen.
//! Measured rather than assumed: on a compositor advertising version seven, a window moved to a
//! workspace nobody is looking at goes on receiving frame callbacks at the full rate of its output
//! and its presentation feedback simply stops being answered, so neither the callbacks nor the
//! feedback nor the outputs it is on say anything at all. Without version six an animation behind
//! such a window runs the whole pipeline for ever.
//!
//! So the shell is bound here, at six, and the window role is built on it directly. Everything
//! else on this backend is still the toolkit's.

pub mod configure;
pub mod popup;
pub mod toplevel;

use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::client::globals::GlobalList;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::decoration::zv1::client::zxdg_decoration_manager_v1::ZxdgDecorationManagerV1;
use wayland_protocols::xdg::shell::client::xdg_wm_base::{self, XdgWmBase};

use crate::driver::WaylandState;

/// The highest version of the shell this backend understands.
///
/// Six for the suspended state. Seven adds only the constrained-edge states, which describe an edge
/// that cannot be dragged outward — nothing above this contract asks — so binding it would be
/// asking for events with no reader.
const VERSION: u32 = 6;

/// The desktop shell and the decoration manager that goes with it.
#[derive(Debug)]
pub struct XdgShell {
    /// The shell itself.
    base: XdgWmBase,
    /// Who draws the frame, where the compositor is willing to.
    decorations: Option<ZxdgDecorationManagerV1>,
}

impl XdgShell {
    /// Binds the shell, or says the compositor has none.
    ///
    /// # Errors
    ///
    /// Returns the bind failure, which for this global means there is no desktop shell at all and
    /// therefore no window this backend can open.
    pub fn bind(
        globals: &GlobalList,
        qh: &QueueHandle<WaylandState>,
    ) -> Result<Self, smithay_client_toolkit::reexports::client::globals::BindError> {
        let base: XdgWmBase = globals.bind(qh, 1..=VERSION, GlobalData)?;
        Ok(Self {
            base,
            decorations: globals.bind(qh, 1..=1, GlobalData).ok(),
        })
    }

    /// The shell object every surface's role is made from.
    pub const fn base(&self) -> &XdgWmBase {
        &self.base
    }

    /// The decoration manager, where the compositor offers one.
    pub const fn decorations(&self) -> Option<&ZxdgDecorationManagerV1> {
        self.decorations.as_ref()
    }

    /// Whether the compositor will draw window frames itself.
    pub const fn draws_frames(&self) -> bool {
        self.decorations.is_some()
    }

    /// Whether the shell is new enough to say when a window has stopped being drawn.
    pub fn reports_suspension(&self) -> bool {
        self.base.version() >= VERSION
    }
}

impl Dispatch<XdgWmBase, GlobalData> for WaylandState {
    fn event(
        _state: &mut Self,
        base: &XdgWmBase,
        event: <XdgWmBase as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Answered here and at once. A compositor that asks whether a client is alive and is not
        // answered concludes that it is not, and kills it — so this must not wait for a turn of the
        // loop, and must not be behind anything that could block.
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}

impl Dispatch<ZxdgDecorationManagerV1, GlobalData> for WaylandState {
    fn event(
        _state: &mut Self,
        _manager: &ZxdgDecorationManagerV1,
        _event: <ZxdgDecorationManagerV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // A factory, which says nothing.
    }
}
