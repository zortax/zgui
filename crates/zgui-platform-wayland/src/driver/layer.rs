//! What the shell says to a layer surface: a configure, or that it closed the surface.
//!
//! The toolkit acknowledges every layer configure as it arrives. A configure the shell sent before
//! the surface unmapped belongs to a mapping that is gone, and a wlroots shell refuses its
//! acknowledgement with a protocol error. So this dispatch is written here: it acknowledges only a
//! configure of the current mapping and drops the others, see `sequence`.

use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, QueueHandle};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, ZwlrLayerSurfaceV1,
};
use smithay_client_toolkit::shell::WaylandSurface as _;
use smithay_client_toolkit::shell::wlr_layer::{
    LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure, LayerSurfaceData,
};

use crate::driver::WaylandState;

smithay_client_toolkit::reexports::client::delegate_dispatch!(WaylandState: [ZwlrLayerShellV1: GlobalData] => LayerShell);

impl LayerShellHandler for WaylandState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        // The reason is decided after the dispatch: the compositor can close the surface before
        // it says the output has gone, in the same batch of events.
        if let Some(id) = self.identify(layer.wl_surface())
            && !self.closing.contains(&id)
        {
            self.closing.push(id);
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        _configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        // Unused: the dispatch below takes every layer configure, and acknowledges it itself.
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, LayerSurfaceData> for WaylandState {
    fn event(
        state: &mut Self,
        wlr: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        data: &LayerSurfaceData,
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(layer) = data.layer_surface() else {
            return;
        };
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => state.layer_configured(wlr, &layer, serial, (width, height)),
            zwlr_layer_surface_v1::Event::Closed => state.closed(conn, qh, &layer),
            _ => {}
        }
    }
}

impl WaylandState {
    /// Acknowledges and applies a configure of the current mapping, and drops any other.
    fn layer_configured(
        &mut self,
        wlr: &ZwlrLayerSurfaceV1,
        layer: &LayerSurface,
        serial: u32,
        size: (u32, u32),
    ) {
        let Some(id) = self.identify(layer.wl_surface()) else {
            // A surface the loop does not track yet is in its first sequence.
            wlr.ack_configure(serial);
            return;
        };
        let current = self
            .live
            .surface(id)
            .is_none_or(|surface| surface.shared().sequence.accepts());
        if !current {
            tracing::debug!(
                ?id,
                serial,
                "dropped a layer configure sent before the unmap"
            );
            return;
        }
        wlr.ack_configure(serial);
        self.sized_by_shell(id, size);
    }
}
