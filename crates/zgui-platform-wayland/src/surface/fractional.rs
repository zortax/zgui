//! Drawing at a scale that is not a whole number.

use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{
    self, WpFractionalScaleV1,
};
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;
use zgui_geom::{Css, CssPx, Size};
use zgui_platform::SurfaceId;

use crate::conn::Extras;
use crate::driver::WaylandState;

/// The two objects a fractionally scaled surface needs, when the compositor offers them.
#[derive(Debug, Default)]
pub struct Fractional {
    /// Where the compositor reports the scale it wants.
    pub scale: Option<WpFractionalScaleV1>,
    /// The mapping from the buffer's extent to the extent the surface appears at.
    pub viewport: Option<WpViewport>,
}

impl Fractional {
    /// Attaches both to `surface`, when this compositor has both.
    pub fn attach(
        extras: &Extras,
        qh: &QueueHandle<WaylandState>,
        surface: &WlSurface,
        id: SurfaceId,
    ) -> Self {
        if !extras.scales_fractionally() {
            return Self::default();
        }
        Self {
            scale: extras
                .fractional_scale
                .as_ref()
                .map(|manager| manager.get_fractional_scale(surface, qh, id)),
            viewport: extras
                .viewporter
                .as_ref()
                .map(|viewporter| viewporter.get_viewport(surface, qh, ())),
        }
    }

    /// Says that a buffer drawn at the surface's scale should appear at `logical`.
    ///
    /// Applied on the commit that carries the matching buffer and never before it. A destination
    /// larger than the buffer it is applied against is a protocol error, so the ordering is not a
    /// preference: the extent is recorded when the compositor configures the surface and sent from
    /// the notification that a frame is about to be committed.
    pub fn destination(&self, logical: Size<CssPx, Css>) {
        let Some(viewport) = &self.viewport else {
            return;
        };
        viewport.set_destination(
            (logical.width.0.round() as i32).max(1),
            (logical.height.0.round() as i32).max(1),
        );
    }

    /// Releases both objects, before the surface they belong to goes.
    pub fn release(&mut self) {
        if let Some(viewport) = self.viewport.take() {
            viewport.destroy();
        }
        if let Some(scale) = self.scale.take() {
            scale.destroy();
        }
    }
}

impl Dispatch<WpFractionalScaleManagerV1, GlobalData> for WaylandState {
    fn event(
        _state: &mut Self,
        _manager: &WpFractionalScaleManagerV1,
        _event: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // The manager is a factory and says nothing.
    }
}

impl Dispatch<WpViewporter, GlobalData> for WaylandState {
    fn event(
        _state: &mut Self,
        _viewporter: &WpViewporter,
        _event: <WpViewporter as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpViewport, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _viewport: &WpViewport,
        _event: <WpViewport as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleV1, SurfaceId> for WaylandState {
    fn event(
        state: &mut Self,
        _scale: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as Proxy>::Event,
        id: &SurfaceId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.scale_changed(*id, |ladder| ladder.fractional(scale));
        }
    }
}
