//! How a surface speaks to the seat, and when it may.

use std::sync::{Mutex, MutexGuard};

use smithay_client_toolkit::reexports::client::Connection;
use smithay_client_toolkit::reexports::client::protocol::wl_seat::WlSeat;
use smithay_client_toolkit::seat::pointer::{PointerData, ThemedPointer};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    ChangeCause, ZwpTextInputV3,
};
use zgui_platform::{CursorStyle, SurfaceId, TextInput, Unsupported};

use crate::input::pointer::cursor;
use crate::input::text_input;

/// The seat, as a surface is allowed to speak to it.
///
/// Five requests in the contract belong to a *surface* and are made against a *seat*: what the
/// pointer looks like, the two interactive drags an application drawing its own title bar needs,
/// and the two that steer the input method. There is one seat whichever surface asks, so each of
/// them is guarded by the same condition — the surface asking has to be the one that currently
/// has the device.
///
/// Without that guard a background window re-rendering its hover state changes the cursor under
/// the window the person is using, a drag quoted against a press that happened somewhere else is
/// declined by the compositor without saying so, and a window nobody is typing into takes the
/// composition away from the one they are.
///
/// Shared and thread-safe because a surface is. The loop writes it; every surface reads it.
#[derive(Debug, Default)]
pub struct SeatLink {
    /// What the loop keeps up to date.
    inner: Mutex<State>,
}

/// What the loop tells the link.
#[derive(Default)]
struct State {
    /// The seat itself, once one exists.
    seat: Option<WlSeat>,
    /// The pointer, with the cursor theme attached to it.
    pointer: Option<ThemedPointer<PointerData>>,
    /// The connection the cursor's own surface is committed on.
    conn: Option<Connection>,
    /// The surface the pointer is over.
    over: Option<SurfaceId>,
    /// The serial an interactive drag may quote, which is always from a press.
    grab: Option<u32>,
    /// What the pointer currently looks like, so that repeating a shape costs nothing.
    style: Option<CursorStyle>,
    /// The input method, once the compositor has offered one for a seat with a keyboard.
    ime: Option<ZwpTextInputV3>,
    /// Which surface the input method is attached to.
    composing_on: Option<SurfaceId>,
    /// Whether a field is currently accepting text.
    enabled: bool,
    /// The caret rectangle last sent, so an unchanged one is not sent again.
    caret: Option<(i32, i32, i32, i32)>,
    /// How many request batches have been committed, which is the serial answers are matched on.
    serial: u32,
    /// The activations asked for and not yet carried out.
    asked: Vec<(SurfaceId, bool)>,
}

impl core::fmt::Debug for State {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("SeatLink")
            .field("over", &self.over)
            .field("composing_on", &self.composing_on)
            .finish_non_exhaustive()
    }
}

impl SeatLink {
    /// Records the seat and the connection its requests are made on.
    pub(crate) fn attach(&self, conn: Connection, seat: Option<WlSeat>) {
        let mut inner = self.lock();
        inner.conn = Some(conn);
        inner.seat = seat;
    }

    /// Takes ownership of the seat's pointer.
    ///
    /// The link owns it rather than borrowing it because it is what makes the requests: a themed
    /// pointer cannot be copied, and every one of the three requests here is made against it.
    pub(crate) fn pointing(&self, pointer: ThemedPointer<PointerData>) {
        let mut inner = self.lock();
        inner.pointer = Some(pointer);
        inner.style = None;
    }

    /// Gives up the seat's pointer, releasing it.
    pub(crate) fn unpointing(&self) {
        let mut inner = self.lock();
        if let Some(pointer) = inner.pointer.take() {
            pointer.pointer().release();
        }
        inner.style = None;
        inner.over = None;
    }

    /// Records the serial an interactive drag may quote.
    pub(crate) fn pressed(&self, serial: u32) {
        self.lock().grab = Some(serial);
    }

    /// Records which surface the pointer is over.
    pub(crate) fn over(&self, surface: Option<SurfaceId>) {
        let mut inner = self.lock();
        if inner.over == surface {
            return;
        }
        inner.over = surface;
        // The next surface to hold the pointer states its own cursor, and the one that lost it
        // must not suppress that by having asked for the same shape earlier.
        inner.style = None;
    }

    /// Sets what the pointer looks like over `surface`, when it is there to be set.
    ///
    /// A shape already in force costs nothing: the runtime states the cursor on every frame that
    /// touches a hover, and each restatement would otherwise be a round trip and, on the themed
    /// path, a buffer.
    pub fn set_cursor(&self, surface: SurfaceId, style: CursorStyle) {
        let mut inner = self.lock();
        if inner.over != Some(surface) || inner.style == Some(style) {
            return;
        }
        inner.style = Some(style);
        let (Some(pointer), Some(conn)) = (&inner.pointer, &inner.conn) else {
            return;
        };
        match cursor::icon(style) {
            Some(icon) => {
                if let Err(error) = pointer.set_cursor(conn, icon) {
                    tracing::debug!(%error, ?style, "the cursor theme has no such shape");
                }
            }
            None => {
                let _ = pointer.hide_cursor();
            }
        }
    }

    /// Records the input method this seat's keyboard was given.
    pub(crate) fn composing_with(&self, ime: Option<ZwpTextInputV3>) {
        let mut inner = self.lock();
        if let Some(old) = inner.ime.take() {
            old.destroy();
        }
        inner.ime = ime;
        inner.enabled = false;
        inner.caret = None;
        inner.composing_on = None;
    }

    /// Records which surface the input method attached itself to.
    pub(crate) fn composing_on(&self, surface: Option<SurfaceId>) {
        let mut inner = self.lock();
        inner.composing_on = surface;
        inner.enabled = false;
        inner.caret = None;
    }

    /// The serial the compositor's next answer must match to still be current.
    pub(crate) fn composition_serial(&self) -> u32 {
        self.lock().serial
    }

    /// Tells the input method what is being typed in `surface`, or that nothing is.
    ///
    /// One call rather than three, because the parts are only meaningful together: an input method
    /// told the caret moved but not that the field is still active places its candidate window over
    /// the text being composed.
    pub fn set_text_input(&self, surface: SurfaceId, state: Option<TextInput>) {
        let mut inner = self.lock();
        if inner.composing_on != Some(surface) {
            return;
        }
        let Some(ime) = inner.ime.clone() else {
            return;
        };
        match state {
            None => {
                if !inner.enabled {
                    return;
                }
                inner.enabled = false;
                inner.caret = None;
                ime.disable();
            }
            Some(state) => {
                let rectangle = text_input::caret(&state);
                // A field that has not moved is not restated: the rectangle crosses as whole
                // numbers, the runtime states the caret on every frame that touches it, and some
                // input methods re-place their candidate window on every request.
                if inner.enabled && !text_input::moved(inner.caret, rectangle) {
                    return;
                }
                if !inner.enabled {
                    ime.enable();
                }
                ime.set_content_type(
                    text_input::hint(state.purpose),
                    text_input::purpose(state.purpose),
                );
                let (x, y, width, height) = rectangle;
                ime.set_cursor_rectangle(x, y, width, height);
                inner.enabled = true;
                inner.caret = Some(rectangle);
            }
        }
        ime.commit();
        inner.serial = inner.serial.wrapping_add(1);
    }

    /// Abandons a half-composed accent so that the next key stands alone.
    ///
    /// The protocol has no reset request, so the composition is ended and restarted — which is what
    /// every toolkit on this desktop does. Leaving it standing means the next key extends text the
    /// field no longer holds.
    pub fn reset_composition(&self) {
        let mut inner = self.lock();
        let Some(ime) = inner.ime.clone() else {
            return;
        };
        if !inner.enabled {
            return;
        }
        ime.set_text_change_cause(ChangeCause::Other);
        ime.disable();
        ime.enable();
        if let Some((x, y, width, height)) = inner.caret {
            ime.set_cursor_rectangle(x, y, width, height);
        }
        ime.commit();
        inner.serial = inner.serial.wrapping_add(1);
    }

    /// Asks the desktop to bring `surface` forward, or to draw attention to it.
    ///
    /// Queued rather than sent, because activation is a two-step conversation with the compositor
    /// and the loop is what holds both ends of it. A frame of delay costs nothing next to the round
    /// trip the token itself takes.
    ///
    /// A desktop is free to refuse either: taking focus from what a person is typing into is the
    /// behaviour focus-stealing prevention exists to stop, and a client cannot tell whether its
    /// request was honoured. Asking is still right — the alternative is a window that never comes
    /// forward even when the person asked it to.
    pub fn activate(&self, surface: SurfaceId, focus: bool) {
        self.lock().asked.push((surface, focus));
    }

    /// Takes every activation asked for since the last turn.
    pub(crate) fn take_activations(&self) -> Vec<(SurfaceId, bool)> {
        std::mem::take(&mut self.lock().asked)
    }

    /// The seat and serial a drag started by `surface` may quote.
    pub fn drag(&self, surface: SurfaceId) -> Result<(WlSeat, u32), Unsupported> {
        let inner = self.lock();
        if inner.over != Some(surface) {
            return Err(Unsupported);
        }
        match (inner.seat.clone(), inner.grab) {
            (Some(seat), Some(serial)) => Ok((seat, serial)),
            _ => Err(Unsupported),
        }
    }

    /// The state, recovering from a panic on another thread.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::SeatLink;
    use zgui_platform::{CursorStyle, SurfaceId};

    #[test]
    fn a_surface_the_pointer_is_not_on_may_not_start_a_drag() {
        // The compositor would decline it anyway, and silently.
        let link = SeatLink::default();
        assert!(link.drag(SurfaceId::new(1)).is_err());
        link.over(Some(SurfaceId::new(2)));
        link.pressed(9);
        assert!(link.drag(SurfaceId::new(1)).is_err());
    }

    #[test]
    fn a_drag_needs_a_press_to_quote_and_not_merely_the_pointer() {
        let link = SeatLink::default();
        link.over(Some(SurfaceId::new(1)));
        assert!(
            link.drag(SurfaceId::new(1)).is_err(),
            "nothing has been pressed yet"
        );
    }

    #[test]
    fn the_pointer_moving_between_surfaces_forgets_the_shape_it_was_showing() {
        // Otherwise the surface it moved to cannot set the shape it had asked for earlier.
        let link = SeatLink::default();
        let first = SurfaceId::new(1);
        link.over(Some(first));
        link.set_cursor(first, CursorStyle::Text);
        link.over(Some(SurfaceId::new(2)));
        link.over(Some(first));
        // With the shape forgotten, asking again reaches the seat rather than being suppressed.
        link.set_cursor(first, CursorStyle::Text);
    }
}
