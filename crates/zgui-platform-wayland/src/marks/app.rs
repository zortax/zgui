//! The marks of the connection.
//!
//! The connection binds the marks global when the compositor advertises it. It then keeps the
//! latency marks of the frame pipeline in memory, so that a present can read its phases. The input
//! handlers take the compositor ids of the events they process and give them to the surface the
//! events went to.

use fakeland_marks::{Family, Marks};
use smithay_client_toolkit::reexports::client::Connection;
use smithay_client_toolkit::reexports::client::globals::GlobalList;
use zgui_platform::SurfaceId;

use crate::driver::WaylandState;
use crate::marks::clock::Anchor;
use crate::surface::WaylandSurface;

/// How many latency marks the ring keeps. Several frames fit, so a held frame still finds the
/// marks of the redraw that composed it.
const RING: usize = 4096;

/// The marks of the connection.
#[derive(Debug)]
pub(crate) struct AppMarks {
    /// The bound global, or an inactive handle.
    marks: Marks,
    /// The clock conversion of the connection.
    anchor: Anchor,
    /// Whether the first configure was marked.
    configured: bool,
}

impl AppMarks {
    /// Binds the marks global from the globals of the connection, and marks the connection.
    pub(crate) fn bind(conn: &Connection, globals: &GlobalList) -> Self {
        let marks = Marks::bind(conn, globals);
        if marks.is_active() {
            // A ring that another reader already keeps stays as it is.
            if !zgui_profile::latency::retaining() {
                zgui_profile::latency::retain_marks(RING);
            }
            // The name is a valid constant, so the call cannot fail.
            let _ = marks.mark("connected");
        }
        Self {
            marks,
            anchor: Anchor::take(),
            configured: false,
        }
    }

    /// Gives a new surface its marks object.
    pub(crate) fn attach(&self, surface: &WaylandSurface) {
        if !self.marks.is_active() {
            return;
        }
        match self.marks.surface(surface.wl_surface()) {
            Ok(marks) => surface.marks().attach(marks, self.anchor),
            Err(error) => tracing::debug!(%error, "a surface has no frame marks"),
        }
    }

    /// Marks the first configure of any surface.
    pub(crate) fn configured(&mut self) {
        if self.configured || !self.marks.is_active() {
            return;
        }
        self.configured = true;
        let _ = self.marks.mark("configured");
    }
}

impl WaylandState {
    /// Gives the ids of the input event that a handler of `family` processed to `surface`.
    pub(crate) fn input_marked(&mut self, surface: Option<SurfaceId>, family: Family) {
        if !self.marks.marks.is_active() {
            return;
        }
        let ids = self.marks.marks.take_inputs_of(family);
        if ids.is_empty() {
            return;
        }
        let Some(surface) = surface.and_then(|id| self.live.surface(id)) else {
            return;
        };
        surface
            .marks()
            .handled(ids, zgui_platform::Clock::now(self.live.clock.as_ref()));
    }
}
