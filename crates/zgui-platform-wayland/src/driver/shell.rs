//! Applying what the compositor asked a surface to become.
//!
//! The shell describes a change in two halves and they must not be acted on separately: the role
//! says what the surface should be, and the surface says the description is complete. So the first
//! half is stashed and this is where the second half applies it, acknowledges it, and reports what
//! actually changed.
//!
//! Everything that leaves this module is an *edge*. A compositor restates: a drag delivers the same
//! extent more than once, an activation is re-sent, and a suspension that has not changed arrives
//! again. Reporting each of those runs the whole pipeline for a frame identical to the one already
//! on the screen, which is the cost this framework exists to avoid.

use zgui_geom::{Css, CssPx, Size};
use zgui_platform::{DecorationSource, Surface as _, SurfaceEvent, SurfaceId};

use crate::driver::WaylandState;
use crate::surface::role::toplevel;
use crate::surface::role::xdg::configure::Pending;

impl WaylandState {
    /// Records what a window's role asked for, without acting on it.
    pub(crate) fn stash_configure(&mut self, id: SurfaceId, pending: Pending) {
        if let Some(surface) = self.live.surface(id) {
            surface.shared().pending_configure = Some(pending);
        }
    }

    /// Records what a pop-up's role asked for, without acting on it.
    ///
    /// A pop-up is told where it ended up as well as how large it is. Where it ended up is the
    /// compositor's business — this desktop tells no surface where it is, and the contract says so
    /// — so only the extent is kept.
    pub(crate) fn stash_popup(&mut self, id: SurfaceId, size: (i32, i32)) {
        if let Some(surface) = self.live.surface(id) {
            surface.shared().pending_configure = Some(Pending::read(size, &[]));
        }
    }

    /// Records who the compositor decided draws the frame.
    ///
    /// The request may be refused, and an application that assumed a server-drawn frame was granted
    /// shows a window with no title bar at all. So the answer is what the capability reports.
    pub(crate) fn decorated_by(&mut self, id: SurfaceId, source: DecorationSource) {
        let _ = id;
        if self.live.capabilities.decorations != source {
            self.live.capabilities.decorations = source;
            tracing::debug!(?source, "the compositor settled who draws window frames");
        }
    }

    /// Applies the configure that has arrived, acknowledges it, and reports what changed.
    ///
    /// The order inside is the protocol's and none of it is optional: acknowledge before the frame
    /// that answers it is committed, and state the window's geometry with the extent it was
    /// answered at. The order of what is *reported* is this framework's — visibility before the
    /// resize, because a surface that has stopped being hidden is redrawn in full and a resize
    /// reported ahead of that would be answered with a partial redraw.
    pub(crate) fn configure_surface(&mut self, id: SurfaceId, serial: u32) {
        let Some(surface) = self.live.surface(id) else {
            return;
        };
        let pending = surface.shared().pending_configure.take();
        let Some(pending) = pending else {
            // A surface acknowledging a description it never received would be acknowledging
            // nothing; the compositor sends the two halves together.
            surface.role().ack(serial);
            return;
        };

        let (resized, visibility, resizing, extent, answered) = {
            let mut shared = surface.shared();
            let extent = toplevel::extent(pending.size, surface.wanted(), shared.logical);
            let state_flip =
                shared.maximized != pending.maximized || shared.fullscreen != pending.fullscreen;
            shared.maximized = pending.maximized;
            shared.fullscreen = pending.fullscreen;
            shared.resizing = pending.resizing;
            shared.visibility.configured = true;
            // The state this whole shell is bound at version six for, and the only signal on this
            // desktop that reliably says a window is not being seen.
            shared.visibility.suspended = pending.suspended;
            if !pending.suspended {
                shared.visibility.answered();
            }
            let scale = shared.ladder.factor();
            let resized = shared.resized(extent, scale);
            let answered = shared.answers_restatement(state_flip);
            (
                resized,
                shared.visibility_edge(),
                shared.resizing_edge(),
                extent,
                answered,
            )
        };

        crate::surface::scale::declare(&surface);
        surface.role().ack(serial);
        if let Some(window) = surface.role().window() {
            window.set_geometry(extent);
        }
        surface.popup_mapped();

        if let Some(event) = visibility {
            self.report(id, event);
        }
        // The two drag edges straddle the resize they ride with. The start edge goes ahead, so
        // the runtime knows the configure that follows belongs to a drag; the release edge goes
        // after, so the level it settles is the one this configure carried.
        let released = matches!(resizing, Some(SurfaceEvent::ResizingChanged(false)));
        if let Some(event) = resizing.filter(|_| !released) {
            zgui_profile::latency::mark("w.resizing");
            self.report(id, event);
        }
        match resized {
            Some(event) => self.report(id, event),
            None if answered => surface.request_redraw(),
            // The rest are restatements. A drag delivers the same extent more than once — a
            // quarter of its configures, measured across a monitor — and each one used to run
            // the whole pipeline for a frame identical to the one already on the screen.
            None => zgui_profile::latency::mark("cfg.repeat"),
        }
        if released {
            zgui_profile::latency::mark("w.resizing");
            self.report(id, SurfaceEvent::ResizingChanged(false));
        }
    }

    /// The extent a surface was last configured at.
    pub(crate) fn configured_extent(&self, id: SurfaceId) -> Option<Size<CssPx, Css>> {
        self.live
            .surface(id)
            .map(|surface| surface.shared().logical)
    }
}
