//! What the compositor says, and where each answer goes.
//!
//! Every handler here does the same two things and nothing else: it updates the surface the event
//! concerns, and it records what the application should be told. None of them calls the
//! application, because they run inside a dispatch and the application is allowed to create and
//! destroy surfaces from its callbacks.

use smithay_client_toolkit::compositor::CompositorHandler;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::client::protocol::wl_callback::{self, WlCallback};
use smithay_client_toolkit::reexports::client::protocol::wl_output::{Transform, WlOutput};
use smithay_client_toolkit::reexports::client::protocol::wl_region::WlRegion;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::{
    delegate_compositor, delegate_output, delegate_registry, delegate_seat, delegate_shm,
    registry_handlers,
};
use zgui_geom::{CssPx, Size};
use zgui_platform::{CloseReason, Surface as _, SurfaceEvent};

use crate::driver::WaylandState;
use crate::output;
use crate::surface::FrameCallback;
use crate::surface::sequence::Barrier;

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

    fn frame(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _: &WlSurface, _: u32) {
        // Unused: every callback is asked for with a `FrameCallback`, which says which of the
        // surface's callbacks it is. See the `Dispatch` below.
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &WlSurface,
        output: &WlOutput,
    ) {
        self.outputs_of(surface, |outputs| {
            if !outputs.contains(output) {
                outputs.push(output.clone());
            }
        });
        self.visibility_changed(surface, |visibility| visibility.entered_output());
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &WlSurface,
        output: &WlOutput,
    ) {
        self.outputs_of(surface, |outputs| outputs.retain(|held| held != output));
        self.visibility_changed(surface, |visibility| visibility.left_output());
    }
}

impl Dispatch<WlCallback, FrameCallback> for WaylandState {
    fn event(
        state: &mut Self,
        _callback: &WlCallback,
        event: wl_callback::Event,
        frame: &FrameCallback,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // The compositor is ready for another frame. This never draws by itself: it clears what
        // the surface owed, and the turn draws if something asked for a frame in the meantime.
        let wl_callback::Event::Done { .. } = event else {
            return;
        };
        // Without presentation feedback, the answer for a committed buffer is the best sign that
        // the compositor showed it.
        let unreported = !state.presentation.reports();
        let events = state.live.surface(frame.surface).map(|surface| {
            let mut shared = surface.shared();
            shared.frame_callback(
                frame.serial,
                zgui_platform::Clock::now(state.live.clock.as_ref()),
            );
            [
                shared.visibility_edge(),
                shared.presentation_edge(unreported),
            ]
        });
        for event in events.into_iter().flatten().flatten() {
            state.report(frame.surface, event);
        }
    }
}

impl Dispatch<WlCallback, Barrier> for WaylandState {
    fn event(
        state: &mut Self,
        _callback: &WlCallback,
        event: wl_callback::Event,
        barrier: &Barrier,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Every configure the shell sent before the unmap has arrived by now.
        let wl_callback::Event::Done { .. } = event else {
            return;
        };
        if let Some(surface) = state.live.surface(barrier.surface) {
            surface.shared().sequence.answered(barrier.serial);
        }
    }
}

impl OutputHandler for WaylandState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
        self.outputs_changed(None);
    }

    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
        self.outputs_changed(None);
    }

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
        // The compositor can remove an output without a leave first. Its rate must stop counting,
        // and a shell layer that was on it closes because it went. A lock surface is told nothing,
        // so it closes here.
        for surface in self.live.surfaces.borrow().iter() {
            let mut shared = surface.shared();
            let held = shared.outputs.contains(&output) || shared.bound.as_ref() == Some(&output);
            shared.output_lost |= held;
            shared.outputs.retain(|held| held != &output);
            if shared.bound.as_ref() == Some(&output)
                && surface.role().lock().is_some()
                && !self.closing.contains(&surface.id())
            {
                self.closing.push(surface.id());
            }
        }
        // The toolkit calls this before it forgets the output, so the output is left out here.
        self.outputs_changed(Some(&output));
    }
}

impl WaylandState {
    /// Applies the extent a shell layer or a lock surface was configured at.
    ///
    /// An axis of zero leaves the extent the application asked for along it. The toolkit has
    /// acknowledged the configure already.
    pub(crate) fn sized_by_shell(&mut self, id: zgui_platform::SurfaceId, size: (u32, u32)) {
        let Some(surface) = self.live.surface(id) else {
            return;
        };
        let (width, height) = size;
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
            // A configure that reaches a surface the application hid answers a commit made before
            // the hide. Drawing into it would map the surface again, so it waits for the show.
            shared.visibility.configured |= !shared.unmapped;
            shared.configured_unshown |= shared.unmapped;
            let scale = shared.ladder.factor();
            let resized = shared.resized(extent, scale);
            // The toolkit acknowledged this configure on arrival, so a lock surface's next commit
            // has to carry a buffer at the new extent. The first configure owes the first buffer.
            if surface.role().lock().is_some() && (resized.is_some() || !shared.mapped) {
                shared.owe_exact_extent();
            }
            (resized, shared.visibility_edge())
        };
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
    ///
    /// `gone` is an output the compositor has removed and the toolkit still lists. The
    /// application is woken only when what it can read changed.
    fn outputs_changed(&mut self, gone: Option<&WlOutput>) {
        let monitors: Vec<_> = self
            .outputs
            .outputs()
            .filter(|output| Some(output) != gone)
            .filter_map(|output| self.outputs.info(&output))
            .map(|info| output::describe(&info))
            .collect();
        let changed = *self.live.monitors.borrow() != monitors;
        if changed {
            *self.live.monitors.borrow_mut() = monitors;
        }
        if changed && self.announce_outputs {
            zgui_platform::Waker::wake(
                self.live.waker.as_ref(),
                zgui_platform::WakeReason::MonitorsChanged,
            );
        }
        let ids: Vec<_> = self
            .live
            .surfaces
            .borrow()
            .iter()
            .map(|surface| surface.id())
            .collect();
        for id in ids {
            self.rescale_from_outputs(id);
            self.declare_rate(id);
        }
    }

    /// Applies a change to the outputs a surface is on, and re-reads the scale and the rate they
    /// give it.
    fn outputs_of(&mut self, surface: &WlSurface, change: impl FnOnce(&mut Vec<WlOutput>)) {
        let Some(id) = self.identify(surface) else {
            return;
        };
        if let Some(held) = self.live.surface(id) {
            change(&mut held.shared().outputs);
        }
        self.rescale_from_outputs(id);
        self.declare_rate(id);
    }

    /// Gives a surface the scale of the outputs it is on.
    ///
    /// Only the outputs it is on: a surface on a 1x output beside a 2x one is drawn at 1x. A
    /// surface on no output yet keeps the scale it was seeded with.
    fn rescale_from_outputs(&mut self, id: zgui_platform::SurfaceId) {
        let Some(held) = self.live.surface(id) else {
            return;
        };
        let infos: Vec<_> = held
            .shared()
            .outputs
            .iter()
            .filter_map(|output| self.outputs.info(output))
            .collect();
        let sharpest = output::mode::sharpest(infos.iter());
        // The layout of the stripes changes what every glyph looks like and moves no extent, so
        // a change asks for a frame of its own.
        let layout =
            zgui_platform::SubpixelLayout::across(infos.iter().map(output::mode::subpixel));
        if std::mem::replace(&mut held.shared().subpixel, layout) != layout {
            zgui_platform::Surface::request_redraw(&*held);
        }
        self.scale_changed(id, |ladder| ladder.outputs(sharpest));
    }

    /// Reports every shell layer the compositor closed during the last dispatch, and every lock
    /// surface whose output went, with its reason.
    ///
    /// Called after the dispatch has finished, because the compositor can close a layer and
    /// remove its output in one batch of events, in either order.
    pub(crate) fn resolve_closings(&mut self) {
        for id in std::mem::take(&mut self.closing) {
            let Some(held) = self.live.surface(id) else {
                continue;
            };
            let reason = if held.shared().output_lost {
                CloseReason::OutputGone
            } else {
                CloseReason::Desktop
            };
            self.report(id, SurfaceEvent::Closed(reason));
        }
    }

    /// Gives a surface the refresh interval of the outputs it is on.
    ///
    /// The timing uses it only while the presentation feedback states no interval, which a
    /// headless output and a variable-refresh output do. Without it, such a surface is paced at
    /// sixty hertz on any output. Of several outputs, the fastest one gives the interval, because
    /// the compositor sends a frame callback each time any of them repaints the surface.
    fn declare_rate(&self, id: zgui_platform::SurfaceId) {
        let Some(held) = self.live.surface(id) else {
            return;
        };
        let mut shared = held.shared();
        let interval = shared
            .outputs
            .iter()
            .filter_map(|output| self.outputs.info(output))
            .filter_map(|info| output::mode::interval(&info))
            .min();
        shared.declares(interval);
    }
}

delegate_compositor!(WaylandState);
delegate_output!(WaylandState);
delegate_seat!(WaylandState);
delegate_shm!(WaylandState);
delegate_registry!(WaylandState);
