//! What the compositor says, and where each answer goes.
//!
//! Every handler here does the same two things and nothing else: it updates the surface the event
//! concerns, and it records what the application should be told. None of them calls the
//! application, because they run inside a dispatch and the application is allowed to create and
//! destroy surfaces from its callbacks.

use smithay_client_toolkit::compositor::CompositorHandler;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::client::protocol::wl_output::{Transform, WlOutput};
use smithay_client_toolkit::reexports::client::protocol::wl_region::WlRegion;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::shell::WaylandSurface as _;
use smithay_client_toolkit::shell::wlr_layer::{
    LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::{
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    delegate_shm, registry_handlers,
};
use zgui_geom::{CssPx, Size};
use zgui_platform::{Surface as _, SurfaceEvent};

use crate::driver::WaylandState;
use crate::output;

impl CompositorHandler for WaylandState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &WlSurface,
        factor: i32,
    ) {
        // The toolkit derives this from the outputs the surface is on. It is the bottom rung of
        // the ladder and is ignored whenever the compositor reports a scale for the surface
        // itself, which `Scale` decides rather than this.
        let Some(id) = self.identify(surface) else {
            return;
        };
        self.scale_changed(id, |ladder| ladder.preferred(factor));
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _transform: Transform,
    ) {
        // A rotated output is composited by the compositor. Nothing above this layer is written in
        // terms of a transformed buffer, so the surface stays upright and the compositor turns it.
    }

    fn frame(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, surface: &WlSurface, _: u32) {
        // The compositor is ready for another frame. This never draws by itself: it clears what
        // the surface owed, and the turn draws if something asked for a frame in the meantime.
        let Some(id) = self.identify(surface) else {
            return;
        };
        let event = self.live.surface(id).and_then(|surface| {
            let mut shared = surface.shared();
            shared.pacer.callback();
            // The compositor spoke about this surface, which ends whatever run of unanswered
            // frames had accumulated — and with it the only occlusion signal that works on every
            // version of the shell.
            shared.visibility.answered();
            shared.visibility_edge()
        });
        if let Some(event) = event {
            self.report(id, event);
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &WlSurface,
        _output: &WlOutput,
    ) {
        self.visibility_changed(surface, |visibility| visibility.entered_output());
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &WlSurface,
        _output: &WlOutput,
    ) {
        self.visibility_changed(surface, |visibility| visibility.left_output());
    }
}

impl OutputHandler for WaylandState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
        self.outputs_changed();
    }

    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
        self.outputs_changed();
    }

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
        self.outputs_changed();
    }
}

impl LayerShellHandler for WaylandState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        let Some(id) = self.identify(layer.wl_surface()) else {
            return;
        };
        self.report(id, SurfaceEvent::CloseRequested);
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(id) = self.identify(layer.wl_surface()) else {
            return;
        };
        let Some(surface) = self.live.surface(id) else {
            return;
        };
        let (width, height) = configure.new_size;
        let (resized, visibility) = {
            let mut shared = surface.shared();
            let wanted = surface.wanted();
            let extent = Size::new(
                if width == 0 {
                    wanted.width
                } else {
                    CssPx(width as f32)
                },
                if height == 0 {
                    wanted.height
                } else {
                    CssPx(height as f32)
                },
            );
            shared.visibility.configured = true;
            let scale = shared.ladder.factor();
            (shared.resized(extent, scale), shared.visibility_edge())
        };
        crate::surface::scale::declare(&surface);
        if let Some(event) = visibility {
            self.report(id, event);
        }
        if let Some(event) = resized {
            self.report(id, event);
        } else {
            surface.request_redraw();
        }
    }
}

impl smithay_client_toolkit::shm::ShmHandler for WaylandState {
    fn shm_state(&mut self) -> &mut smithay_client_toolkit::shm::Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for WaylandState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

impl Dispatch<WlRegion, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _region: &WlRegion,
        _event: <WlRegion as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // A region is write-only; it says nothing back.
    }
}

impl WaylandState {
    /// Which surface a protocol object belongs to, while it still exists.
    pub(crate) fn identify(&self, surface: &WlSurface) -> Option<zgui_platform::SurfaceId> {
        self.live
            .surfaces
            .borrow()
            .iter()
            .find(|held| held.wl_surface() == surface)
            .map(|held| held.id())
    }

    /// Applies a change to what the compositor has said about a surface being drawn.
    fn visibility_changed(
        &mut self,
        surface: &WlSurface,
        change: impl FnOnce(&mut crate::frame::Visibility),
    ) {
        let Some(id) = self.identify(surface) else {
            return;
        };
        let Some(held) = self.live.surface(id) else {
            return;
        };
        let event = {
            let mut shared = held.shared();
            change(&mut shared.visibility);
            shared.visibility_edge()
        };
        if let Some(event) = event {
            self.report(id, event);
        }
    }

    /// Rebuilds what is known about the outputs, and re-reads every surface's scale from them.
    fn outputs_changed(&mut self) {
        let infos: Vec<_> = self
            .outputs
            .outputs()
            .filter_map(|output| self.outputs.info(&output))
            .collect();
        let sharpest = output::mode::sharpest(infos.iter());
        *self.live.monitors.borrow_mut() = infos.iter().map(output::describe).collect();
        let ids: Vec<_> = self
            .live
            .surfaces
            .borrow()
            .iter()
            .map(|surface| surface.id())
            .collect();
        for id in ids {
            self.scale_changed(id, |ladder| ladder.outputs(sharpest));
        }
    }
}

delegate_compositor!(WaylandState);
delegate_output!(WaylandState);
delegate_seat!(WaylandState);
delegate_shm!(WaylandState);
delegate_registry!(WaylandState);
delegate_layer!(WaylandState);
