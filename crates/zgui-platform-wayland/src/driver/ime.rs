//! What the input method says, and when it is still describing the field that is there.
//!
//! The protocol is a request-and-acknowledge one rather than a stream. Every batch of requests ends
//! with a commit that carries an implicit serial; every change the input method makes arrives as
//! several events followed by a `done` carrying the serial it was made against; and a `done` whose
//! serial is not the latest describes a field this application has already replaced. Applying one
//! of those commits text into a place the person has left.
//!
//! The requests live on the [seat link](crate::surface::SeatLink), because a surface makes them.
//! What lives here is the receiving half: the events, accumulated until the compositor says the
//! change is complete.

use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::client::protocol::wl_seat::WlSeat;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{self, ZwpTextInputV3};
use zgui_platform::{SurfaceEvent, SurfaceId};
use zgui_vocab::ImeEvent;

use crate::driver::WaylandState;
use crate::input::text_input::Composing;

/// The receiving half of the input method.
#[derive(Debug, Default)]
pub(crate) struct Ime {
    /// The surface it attached itself to.
    pub(crate) surface: Option<SurfaceId>,
    /// The change being accumulated until the compositor says it is complete.
    pub(crate) pending: Composing,
}

impl WaylandState {
    /// Opens the input method for `seat`, when the compositor offers one.
    ///
    /// Opened with the keyboard rather than with the seat: there is nothing to compose into on a
    /// seat that cannot type.
    pub(crate) fn open_ime(&mut self, seat: &WlSeat) {
        let Some(manager) = &self.text_input_manager else {
            return;
        };
        let input = manager.get_text_input(seat, &self.qh, ());
        self.link.composing_with(Some(input));
    }

    /// Closes it, because the seat lost its keyboard.
    pub(crate) fn close_ime(&mut self) {
        self.link.composing_with(None);
        self.ime = Ime::default();
    }
}

impl Dispatch<ZwpTextInputManagerV3, GlobalData> for WaylandState {
    fn event(
        _state: &mut Self,
        _manager: &ZwpTextInputManagerV3,
        _event: <ZwpTextInputManagerV3 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // A factory, which says nothing.
    }
}

impl Dispatch<ZwpTextInputV3, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _input: &ZwpTextInputV3,
        event: <ZwpTextInputV3 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::Enter { surface } => state.ime_entered(&surface),
            zwp_text_input_v3::Event::Leave { .. } => state.ime_left(),
            zwp_text_input_v3::Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                state.ime.pending.preedit =
                    text.map(|text| (text, Some((cursor_begin, cursor_end))));
            }
            zwp_text_input_v3::Event::CommitString { text } => state.ime.pending.commit = text,
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => state.ime.pending.delete = Some((before_length, after_length)),
            zwp_text_input_v3::Event::Done { serial } => state.ime_done(serial),
            _ => {}
        }
    }
}

impl WaylandState {
    /// The input method attached itself to a surface.
    fn ime_entered(&mut self, surface: &WlSurface) {
        let Some(id) = self.identify(surface) else {
            return;
        };
        self.ime.surface = Some(id);
        self.link.composing_on(Some(id));
        self.report(id, SurfaceEvent::Ime(ImeEvent::Enabled));
    }

    /// The input method let go of the surface it was on.
    fn ime_left(&mut self) {
        let Some(id) = self.ime.surface.take() else {
            return;
        };
        self.link.composing_on(None);
        self.ime.pending = Composing::default();
        // Whatever was being composed is abandoned, and the field has to be told or the provisional
        // text stays on screen with nothing left to finish it.
        self.report(id, SurfaceEvent::Ime(ImeEvent::Disabled));
    }

    /// The compositor says a change is complete.
    fn ime_done(&mut self, serial: u32) {
        let step = std::mem::take(&mut self.ime.pending);
        let Some(id) = self.ime.surface else {
            return;
        };
        if serial != self.link.composition_serial() {
            tracing::debug!(
                answered = serial,
                current = self.link.composition_serial(),
                "an input method change arrived against a field that had already been replaced"
            );
            return;
        }
        if step.is_empty() {
            return;
        }
        if let Some((before, after)) = step.deletion() {
            // The contract has no event for deleting the text around the caret, and inventing one
            // here would be a change nothing above knows how to apply. Recorded so that the
            // languages which need it are a known gap rather than a silent one.
            tracing::debug!(
                before,
                after,
                "the input method asked to delete surrounding text, which this contract cannot carry"
            );
        }
        for event in step.events() {
            self.report(id, SurfaceEvent::Ime(event));
        }
    }
}
