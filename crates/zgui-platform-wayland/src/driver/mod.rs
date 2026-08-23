//! The loop, and the state every protocol handler is written on.

mod activation;
mod data;
mod ime;
mod input;
pub mod live;
mod protocol;
mod shell;
mod surfaces;
mod turn;

pub use crate::driver::turn::{WaylandApp, run};

use std::sync::Arc;
use std::time::Duration;

use smithay_client_toolkit::activation::ActivationState;
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::data_device_manager::DataDeviceManagerState;
use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::primary_selection::PrimarySelectionManagerState;
use smithay_client_toolkit::reexports::client::globals::GlobalList;
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use smithay_client_toolkit::registry::RegistryState;
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::shell::wlr_layer::LayerShell;
use smithay_client_toolkit::shm::Shm;
use zgui_platform::{PlatformError, SurfaceEvent, SurfaceId};

use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3;

use crate::capabilities::{self, Offered};
use crate::clipboard::WaylandClipboard;
use crate::clock::Monotonic;
use crate::conn::Extras;
use crate::driver::ime::Ime;
use crate::driver::live::Live;
use crate::frame::Presentation;
use crate::input::{Drag, Seat, touch::Contacts};
use crate::surface::Scale;
use crate::waker::PingWaker;

/// Everything the loop holds, and the one type every protocol handler is implemented on.
///
/// The toolkit dispatches protocol events into a single state type, so there is exactly one of
/// these. That does not make it one file: each handler is written beside the feature it serves,
/// and this is only where the fields live.
///
/// # Nothing reaches the application from inside a dispatch
///
/// A protocol event never calls the application. It records what happened in [`WaylandState::out`]
/// and returns, and the turn delivers the batch afterwards with nothing borrowed. That is what
/// makes the borrowed context safe to hand out at all: the application can create a surface, close
/// one, or ask for the clipboard from inside a callback, and none of it can re-enter a dispatch
/// that is still running.
pub struct WaylandState {
    /// The connection everything is created on.
    pub(crate) conn: Connection,
    /// The queue everything is dispatched on.
    pub(crate) qh: QueueHandle<Self>,
    /// The globals, as the toolkit tracks them.
    pub(crate) registry: RegistryState,
    /// What is known about the outputs.
    pub(crate) outputs: OutputState,
    /// The compositor, which makes surfaces and regions.
    pub(crate) compositor: CompositorState,
    /// The desktop shell, which makes windows and pop-ups.
    pub(crate) xdg: Arc<crate::surface::role::xdg::XdgShell>,
    /// The shell layer, where the compositor offers one.
    pub(crate) layers: Option<LayerShell>,
    /// The seats, and everything attached to them.
    pub(crate) seats: SeatState,
    /// The one seat this backend tracks, and what it is pointed at.
    pub(crate) seat: Seat,
    /// The seat, as a surface is allowed to speak to it.
    pub(crate) link: Arc<crate::surface::SeatLink>,
    /// Where each finger is.
    pub(crate) contacts: Contacts,
    /// Whether a continuous scroll is in progress.
    pub(crate) gesturing: bool,
    /// Shared memory, which the cursor theme's images are read out of.
    pub(crate) shm: Shm,
    /// The loop this backend's own timers are registered on.
    pub(crate) events: calloop::LoopHandle<'static, Self>,
    /// Where a frame's presentation is reported.
    pub(crate) presentation: Presentation,
    /// The globals the toolkit has no wrapper for.
    pub(crate) extras: Extras,
    /// The desktop's clipboards.
    pub(crate) clipboard: WaylandClipboard,
    /// What is being dragged over a window, while something is.
    pub(crate) drag: Drag,
    /// The input method, and what it has been told.
    pub(crate) ime: Ime,
    /// How a window asks the desktop to bring it forward.
    pub(crate) activation: Option<ActivationState>,
    /// What each surface asked the desktop for, until its token arrives.
    pub(crate) wanted: std::collections::HashMap<SurfaceId, activation::Wanted>,
    /// The factory the input method is made from, when the compositor offers one.
    pub(crate) text_input_manager: Option<ZwpTextInputManagerV3>,
    /// The clipboard factories, bound at start-up and used once a seat exists.
    ///
    /// The managers are global and the devices are per seat, so the two halves are opened at
    /// different moments: these here, and the devices when the compositor advertises a seat.
    pub(crate) data: Option<DataDeviceManagerState>,
    /// The same for the selection clipboard, which a compositor may not have at all.
    pub(crate) primary: Option<PrimarySelectionManagerState>,
    /// The compositor's clock, placed on this process's own timeline.
    pub(crate) monotonic: Monotonic,
    /// What a callback may change.
    pub(crate) live: Live,
    /// What happened during a dispatch and has not been delivered yet.
    pub(crate) out: Vec<(SurfaceId, SurfaceEvent)>,
}

impl WaylandState {
    /// Binds everything this backend needs on `globals`.
    ///
    /// The two required globals are the compositor and the desktop shell. Everything else is
    /// optional and turns into a capability rather than into a failure, because a compositor
    /// without a clipboard is still a compositor a window can open on.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when a required global is missing, naming it.
    pub fn new(
        conn: Connection,
        globals: &GlobalList,
        qh: QueueHandle<Self>,
        events: calloop::LoopHandle<'static, Self>,
        waker: Arc<PingWaker>,
    ) -> Result<Self, PlatformError> {
        let compositor = CompositorState::bind(globals, &qh)
            .map_err(|error| PlatformError::Backend(format!("wl_compositor: {error}")))?;
        // Required rather than optional because the cursor theme is read through it, and a window
        // whose pointer never changes shape is a window that cannot show what is draggable.
        let shm = Shm::bind(globals, &qh)
            .map_err(|error| PlatformError::Backend(format!("wl_shm: {error}")))?;
        let xdg = crate::surface::role::xdg::XdgShell::bind(globals, &qh)
            .map(Arc::new)
            .map_err(|error| PlatformError::Backend(format!("xdg_wm_base: {error}")))?;
        let layers = LayerShell::bind(globals, &qh).ok();
        let extras = Extras::bind(globals, &qh);
        let presentation = Presentation::bind(globals, &qh);
        let text_input: Option<ZwpTextInputManagerV3> = globals.bind(&qh, 1..=1, GlobalData).ok();
        let activation = ActivationState::bind(globals, &qh).ok();
        let data = DataDeviceManagerState::bind(globals, &qh).ok();
        let primary = PrimarySelectionManagerState::bind(globals, &qh).ok();

        let offered = Offered {
            layer_shell: layers.is_some(),
            clipboard: data.is_some(),
            primary_selection: primary.is_some(),
            text_input: text_input.is_some(),
            // The toolkit asks for a server-drawn frame per window and the compositor answers per
            // window, so this is what it can do rather than what it did; a window that is told
            // otherwise reports it for itself.
            server_decorations: true,
            ..Offered::default()
        };
        let mut state = Self {
            conn,
            registry: RegistryState::new(globals),
            outputs: OutputState::new(globals, &qh),
            compositor,
            xdg,
            layers,
            seats: SeatState::new(globals, &qh),
            seat: Seat::new(),
            link: Arc::default(),
            contacts: Contacts::default(),
            gesturing: false,
            shm,
            events,
            presentation,
            extras,
            clipboard: WaylandClipboard::default(),
            drag: Drag::default(),
            ime: Ime::default(),
            activation,
            wanted: std::collections::HashMap::new(),
            text_input_manager: text_input,
            data,
            primary,
            monotonic: Monotonic::anchor(),
            live: Live::new(capabilities::of(offered), Arc::clone(&waker)),
            out: Vec::new(),
            qh: qh.clone(),
        };
        state.clipboard.selections().attach(
            state.conn.clone(),
            qh,
            Arc::clone(&waker) as Arc<dyn zgui_platform::Waker>,
        );
        // The preference lives on the session bus rather than in the protocol, and is watched from
        // here on so that a person switching their desktop to dark is answered by a redraw rather
        // than by nothing until the next launch.
        let watching = crate::theme::watch(
            Arc::clone(&waker) as Arc<dyn zgui_platform::Waker>,
            Arc::clone(&state.live.scheme),
        );
        state.live.capabilities = capabilities::of(Offered {
            color_scheme: watching,
            ..offered
        });
        // The seats that already exist are bound by the toolkit without being announced, so the
        // one this backend speaks for is taken here rather than waiting to be told about it. Its
        // capabilities arrive on the first dispatch and open the devices themselves.
        if let Some(seat) = state.seats.seats().next() {
            state.take_seat(&seat);
        }
        Ok(state)
    }

    /// Starts reading the paths of a drag that has just arrived.
    ///
    /// On a thread, for the reason every transfer here is: the other end is another process.
    pub(crate) fn start_drag_read(&mut self) {
        let Some(offer) = self.drag.to_read() else {
            return;
        };
        let has_files = offer.with_mime_types(crate::clipboard::mime::has_files);
        if !has_files {
            return;
        }
        // Accepted before it is asked for, which the protocol requires and which is also what
        // tells the source this window is a target at all.
        offer.accept_mime_type(
            offer.serial,
            Some(crate::clipboard::mime::URI_LIST.to_owned()),
        );
        let Ok(reader) = offer.receive(crate::clipboard::mime::URI_LIST.to_owned()) else {
            return;
        };
        let _ = self.conn.flush();
        let answers = self.drag.answers();
        let waker = Arc::clone(&self.live.waker);
        let _ = std::thread::Builder::new()
            .name("zgui-drop".to_owned())
            .spawn(move || {
                let paths = crate::clipboard::pipe::read(reader, crate::clipboard::pipe::PATIENCE)
                    .map(|bytes| crate::input::dnd::paths(&bytes))
                    .unwrap_or_default();
                *answers.lock().unwrap_or_else(|held| held.into_inner()) = Some(paths);
                // The loop is parked on the socket and the answer arrived on a pipe, so it has to
                // be told. What it does with the wake is drain the answer and report the drag.
                zgui_platform::Waker::wake(waker.as_ref(), zgui_platform::WakeReason::AppWork);
            });
    }

    /// Reports a drag whose paths have just arrived, if one was waiting for them.
    pub(crate) fn drag_read_finished(&mut self) {
        let paths = self
            .drag
            .answers()
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone();
        let Some(paths) = paths else {
            return;
        };
        if let Some((id, event)) = self.drag.read_finished(paths) {
            self.report(id, zgui_platform::SurfaceEvent::Drag(event));
        }
    }

    /// Why the loop stopped, in the compositor's words where it left any.
    ///
    /// A loop that fails reports an input-output error and nothing else, because by the time the
    /// socket breaks the reason has already been sent down it. The connection keeps that reason,
    /// and it is the difference between "something went wrong" and the interface, the request and
    /// the rule that was broken — which is the whole of what is needed to fix a protocol defect.
    pub(crate) fn explain(&self, error: &calloop::Error) -> PlatformError {
        match self.conn.protocol_error() {
            Some(protocol) => PlatformError::Backend(format!(
                "the compositor refused a request on {}#{} (error {}): {}",
                protocol.object_interface, protocol.object_id, protocol.code, protocol.message
            )),
            None => PlatformError::Backend(format!("the wayland loop: {error}")),
        }
    }

    /// Records something that happened to a surface, for delivery after this dispatch.
    pub(crate) fn report(&mut self, id: SurfaceId, event: SurfaceEvent) {
        self.out.push((id, event));
    }

    /// Records which clock the compositor times presentation against.
    pub(crate) fn presentation_clock(&mut self, id: u32) {
        self.presentation.clock(id);
    }

    /// Records a frame reaching the screen.
    pub(crate) fn frame_presented(
        &mut self,
        id: SurfaceId,
        seconds: u64,
        nanoseconds: u32,
        refresh: Duration,
    ) {
        let Some(surface) = self.live.surface(id) else {
            return;
        };
        let at = self.monotonic.instant(seconds, nanoseconds);
        let event = {
            let mut shared = surface.shared();
            shared.presented(at, refresh);
            shared.visibility.answered();
            shared.visibility_edge()
        };
        if let Some(event) = event {
            self.report(id, event);
        }
    }

    /// Records a frame the compositor never showed.
    ///
    /// Still an answer: the compositor spoke about this surface, which is what the run of
    /// unanswered frames counts. A discarded frame says the content was superseded, not that the
    /// window is gone.
    pub(crate) fn frame_discarded(&mut self, id: SurfaceId) {
        let event = self.live.surface(id).and_then(|surface| {
            let mut shared = surface.shared();
            shared.timing.discarded();
            shared.visibility.answered();
            shared.visibility_edge()
        });
        if let Some(event) = event {
            self.report(id, event);
        }
    }

    /// Applies a change to what scale a surface should be drawn at.
    ///
    /// One entry point for all three sources, because they are a ladder rather than alternatives:
    /// whichever one moved, the answer is recomputed from the whole ladder and reported only if it
    /// actually changed.
    pub(crate) fn scale_changed(&mut self, id: SurfaceId, change: impl FnOnce(&mut Scale)) {
        let Some(surface) = self.live.surface(id) else {
            return;
        };
        let event = {
            let mut shared = surface.shared();
            change(&mut shared.ladder);
            let scale = shared.ladder.factor();
            let logical = shared.logical;
            shared.resized(logical, scale)
        };
        crate::surface::scale::declare(&surface);
        if let Some(event) = event {
            self.report(id, event);
        }
    }
}

impl core::fmt::Debug for WaylandState {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WaylandState")
            .field("live", &self.live)
            .field("pending", &self.out.len())
            .finish_non_exhaustive()
    }
}
