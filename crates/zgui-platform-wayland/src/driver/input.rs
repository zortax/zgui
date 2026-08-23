//! What the seat says, and where each event goes.
//!
//! Like the rest of the protocol handlers, none of these calls the application: each translates
//! what arrived and records it for the turn to deliver. What is different here is that every one
//! of them also has to keep a *level* up to date — the held modifiers, the pointer's position, who
//! has focus — because the protocol reports changes and the contract carries states.

use smithay_client_toolkit::reexports::client::protocol::wl_keyboard::WlKeyboard;
use smithay_client_toolkit::reexports::client::protocol::wl_pointer::WlPointer;
use smithay_client_toolkit::reexports::client::protocol::wl_seat::WlSeat;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::protocol::wl_touch::WlTouch;
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent as WaylandKey, KeyboardHandler, Keysym, Modifiers as Held, RawModifiers, RepeatInfo,
};
use smithay_client_toolkit::seat::pointer::{
    PointerEvent, PointerEventKind, PointerHandler, ThemeSpec,
};
use smithay_client_toolkit::seat::touch::TouchHandler;
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::{delegate_keyboard, delegate_pointer, delegate_touch};
use zgui_platform::{SurfaceEvent, SurfaceId};
use zgui_vocab::{KeyState, PointerAction};

use crate::driver::WaylandState;
use crate::input::keyboard::{self, modifiers};
use crate::input::pointer::{axis, button};
use crate::input::seat::Adopted;

impl SeatHandler for WaylandState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, seat: WlSeat) {
        // Only for a seat plugged in while the program is running. The seat that already existed
        // when it started is never announced here, which is why nothing may depend on this alone.
        self.take_seat(&seat);
    }

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        // Adopted here rather than merely checked. This is the first place an ordinary desktop's
        // only seat is ever seen: the toolkit binds the seats that already exist without
        // announcing them, so a backend that waited to be told would open nothing, ever.
        if !self.take_seat(&seat).is_ours() {
            return;
        }
        match capability {
            Capability::Keyboard if self.seat.keyboard.is_none() => {
                self.seat.offered.keyboard = true;
                // Repeat comes from the loop's own timer rather than from held keys, because the
                // compositor states a rate and a delay and expects the client to honour them.
                match self.seats.get_keyboard_with_repeat(
                    qh,
                    &seat,
                    None,
                    self.events.clone(),
                    Box::new(|state: &mut Self, _keyboard: &WlKeyboard, event| {
                        state.key_repeated(&event);
                    }),
                ) {
                    Ok(keyboard) => {
                        self.seat.keyboard = Some(keyboard);
                        // The input method belongs to a seat with a keyboard: there is nothing to
                        // compose into on a seat that cannot type.
                        self.open_ime(&seat);
                    }
                    Err(error) => tracing::warn!(%error, "the seat's keyboard could not be opened"),
                }
            }
            Capability::Pointer if !self.seat.pointing => {
                self.seat.offered.pointer = true;
                let surface = self.compositor.create_surface(qh);
                match self.seats.get_pointer_with_theme(
                    qh,
                    &seat,
                    self.shm.wl_shm(),
                    surface,
                    ThemeSpec::default(),
                ) {
                    Ok(pointer) => {
                        self.link.pointing(pointer);
                        self.seat.pointing = true;
                    }
                    Err(error) => tracing::warn!(%error, "the seat's pointer could not be opened"),
                }
            }
            Capability::Touch if self.seat.touch.is_none() => {
                self.seat.offered.touch = true;
                match self.seats.get_touch(qh, &seat) {
                    Ok(touch) => self.seat.touch = Some(touch),
                    Err(error) => tracing::warn!(%error, "the seat's touch could not be opened"),
                }
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: WlSeat,
        capability: Capability,
    ) {
        // A device unplugged mid-gesture leaves whatever it was doing unfinished, so the levels it
        // owned are cleared with it: a held modifier nobody will release is a keyboard that types
        // in capitals for ever.
        match capability {
            Capability::Keyboard => {
                self.seat.offered.keyboard = false;
                if let Some(keyboard) = self.seat.keyboard.take() {
                    keyboard.release();
                }
                self.seat.held = zgui_vocab::Modifiers::NONE;
                self.close_ime();
                self.blur();
            }
            Capability::Pointer => {
                self.seat.offered.pointer = false;
                self.link.unpointing();
                self.seat.pointing = false;
                self.seat.pointer_focus = None;
            }
            Capability::Touch => {
                self.seat.offered.touch = false;
                if let Some(touch) = self.seat.touch.take() {
                    touch.release();
                }
                self.cancel_contacts();
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, conn: &Connection, qh: &QueueHandle<Self>, seat: WlSeat) {
        if self.seat.serials.seat.as_ref() != Some(&seat) {
            return;
        }
        for capability in [Capability::Keyboard, Capability::Pointer, Capability::Touch] {
            self.remove_capability(conn, qh, seat.clone(), capability);
        }
        self.seat.serials.seat = None;
        self.link.attach(self.conn.clone(), None);
    }
}

impl KeyboardHandler for WaylandState {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        surface: &WlSurface,
        serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
        self.observed(serial);
        // The keys already down when focus arrived are deliberately not replayed. Dispatching them
        // would type characters nobody pressed into whatever has just been focused.
        let id = self.identify(surface);
        self.focus_moved(id);
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _surface: &WlSurface,
        serial: u32,
    ) {
        self.observed(serial);
        self.blur();
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        serial: u32,
        event: WaylandKey,
    ) {
        self.pressed(serial);
        self.key(KeyState::Pressed, &event, false);
    }

    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _serial: u32,
        event: WaylandKey,
    ) {
        // Marked as a repeat rather than dispatched as a fresh press: holding a letter down should
        // insert another letter and must not run a command a second time, and only what is being
        // told about the press knows which of those it is doing.
        self.key(KeyState::Pressed, &event, true);
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        serial: u32,
        event: WaylandKey,
    ) {
        self.observed(serial);
        self.key(KeyState::Released, &event, false);
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        serial: u32,
        held: Held,
        _raw: RawModifiers,
        _layout: u32,
    ) {
        self.observed(serial);
        let held = modifiers(held);
        if self.seat.held == held {
            return;
        }
        self.seat.held = held;
        // Reported even without a key event, because a modifier can be pressed or released while
        // the surface is not focused, and a set recovered only from key events is then wrong until
        // the next press.
        if let Some(id) = self.seat.keyboard_focus {
            self.report(id, SurfaceEvent::ModifiersChanged(held));
        }
    }

    fn update_repeat_info(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _info: RepeatInfo,
    ) {
        // The toolkit's repeat source honours this itself; nothing above the contract asks.
    }
}

impl PointerHandler for WaylandState {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            self.pointer_event(event);
        }
    }
}

impl TouchHandler for WaylandState {
    fn down(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &WlTouch,
        serial: u32,
        _time: u32,
        surface: WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        self.pressed(serial);
        let Some(target) = self.identify(&surface) else {
            return;
        };
        let at = crate::input::pointer::position(position.0, position.1);
        self.contacts.down(id, target, at);
        self.contact(target, id, at, PointerAction::Pressed);
    }

    fn up(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &WlTouch,
        serial: u32,
        _time: u32,
        id: i32,
    ) {
        self.observed(serial);
        // The release carries no position, so it lands where the finger last was.
        let Some((target, at)) = self.contacts.up(id) else {
            return;
        };
        self.contact(target, id, at, PointerAction::Released);
    }

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &WlTouch,
        _time: u32,
        id: i32,
        position: (f64, f64),
    ) {
        let at = crate::input::pointer::position(position.0, position.1);
        let Some(target) = self.contacts.moved(id, at) else {
            return;
        };
        self.contact(target, id, at, PointerAction::Moved);
    }

    fn shape(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &WlTouch,
        _id: i32,
        _major: f64,
        _minor: f64,
    ) {
        // The contact patch's ellipse has no reader above the contract.
    }

    fn orientation(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &WlTouch,
        _id: i32,
        _orientation: f64,
    ) {
    }

    fn cancel(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _touch: &WlTouch) {
        self.cancel_contacts();
    }
}

impl WaylandState {
    /// Takes `seat` as the one this backend speaks for, opening what belongs to a seat.
    ///
    /// Everything a seat owns and a surface does not — the clipboards, the way a window asks to be
    /// brought forward — is opened the first time one is seen, wherever that happens to be.
    pub(crate) fn take_seat(&mut self, seat: &WlSeat) -> Adopted {
        let adopted = self.seat.adopt(seat);
        if adopted == Adopted::Now {
            self.link.attach(self.conn.clone(), Some(seat.clone()));
            self.open_data_devices(seat);
        }
        adopted
    }

    /// Records a serial from something the user pressed.
    ///
    /// One entry point for all three of the things that need one, because they need the same one:
    /// a compositor grants a pop-up grab, an interactive drag and a claim on a selection only
    /// against a serial from a press, and quoting anything else is declined silently.
    pub(crate) fn pressed(&mut self, serial: u32) {
        self.seat.serials.pressed(serial);
        self.link.pressed(serial);
        self.clipboard.selections().observed(serial);
    }

    /// Records a serial from something that was not a press.
    ///
    /// A pop-up grab and an interactive drag may not quote one; a claim on a selection may, and
    /// has to — a copy on a window that has just been focused with the keyboard has no press to
    /// point at, and refusing it means a copy that silently does nothing.
    fn observed(&mut self, serial: u32) {
        self.seat.serials.observed(serial);
        self.clipboard.selections().observed(serial);
    }

    /// Tells a surface's accessibility channel whether it has the keyboard.
    ///
    /// A screen reader follows the focused window, and one that is never told stays pointed at
    /// whichever window was focused when it attached.
    fn a11y_focus(&self, surface: SurfaceId, focused: bool) {
        if let Some(surface) = self.live.surface(surface) {
            surface.a11y().focused(focused);
        }
    }

    /// Reports a key, to whichever surface has the keyboard.
    fn key(&mut self, state: KeyState, event: &WaylandKey, repeat: bool) {
        let Some(id) = self.seat.keyboard_focus else {
            return;
        };
        let timestamp = zgui_platform::Clock::timestamp(self.live.clock.as_ref());
        self.report(
            id,
            SurfaceEvent::Key {
                state,
                event: keyboard::event(event, repeat),
                modifiers: self.seat.held,
                timestamp,
            },
        );
    }

    /// Reports a press the repeat timer produced.
    pub(crate) fn key_repeated(&mut self, event: &WaylandKey) {
        self.key(KeyState::Pressed, event, true);
    }

    /// Moves the keyboard focus, telling both surfaces that are affected.
    fn focus_moved(&mut self, surface: Option<SurfaceId>) {
        let Some((left, gained)) = self.seat.focus(surface) else {
            return;
        };
        if let Some(left) = left.filter(|left| Some(*left) != surface) {
            self.a11y_focus(left, false);
            self.report(left, SurfaceEvent::Focused(false));
        }
        if surface.is_some() {
            self.a11y_focus(gained, true);
            self.report(gained, SurfaceEvent::Focused(true));
        }
    }

    /// Gives up the keyboard focus.
    fn blur(&mut self) {
        self.focus_moved(None);
    }

    /// Ends every live contact, for a gesture the compositor took away.
    fn cancel_contacts(&mut self) {
        for (id, target, at) in self.contacts.cancel() {
            // Reported as a release rather than dropped: a contact left down is a control that
            // stays pressed for ever.
            self.contact(target, id, at, PointerAction::Released);
        }
    }

    /// Reports one contact doing one thing.
    fn contact(
        &mut self,
        surface: SurfaceId,
        id: i32,
        at: zgui_geom::Point<zgui_geom::CssPx, zgui_geom::Css>,
        action: PointerAction,
    ) {
        let timestamp = zgui_platform::Clock::timestamp(self.live.clock.as_ref());
        self.report(
            surface,
            SurfaceEvent::Pointer {
                action,
                event: crate::input::pointer::touch(id, at),
                modifiers: self.seat.held,
                timestamp,
            },
        );
    }

    /// Reports one thing the pointer did.
    fn pointer_event(&mut self, event: &PointerEvent) {
        let Some(id) = self.identify(&event.surface) else {
            return;
        };
        let at = crate::input::pointer::position(event.position.0, event.position.1);
        self.seat.moved(at);
        let timestamp = zgui_platform::Clock::timestamp(self.live.clock.as_ref());
        let held = self.seat.held;

        let reported = match &event.kind {
            PointerEventKind::Enter { serial } => {
                self.observed(*serial);
                self.seat.pointer_focus = Some(id);
                self.link.over(Some(id));
                // No motion follows an enter unless the pointer keeps moving, so one is made here:
                // without it nothing under the pointer knows it is being hovered.
                Some((PointerAction::Entered, None))
            }
            PointerEventKind::Leave { serial } => {
                self.observed(*serial);
                self.seat.pointer_focus = None;
                self.link.over(None);
                Some((PointerAction::Left, None))
            }
            PointerEventKind::Motion { .. } => Some((PointerAction::Moved, None)),
            PointerEventKind::Press {
                button: code,
                serial,
                ..
            } => {
                self.seat.serials.pressed(*serial);
                self.link.pressed(*serial);
                Some((PointerAction::Pressed, Some(button::button(*code))))
            }
            PointerEventKind::Release {
                button: code,
                serial,
                ..
            } => {
                self.observed(*serial);
                Some((PointerAction::Released, Some(button::button(*code))))
            }
            PointerEventKind::Axis {
                horizontal,
                vertical,
                source,
                ..
            } => {
                self.wheel(id, horizontal, vertical, *source, at, held, timestamp);
                None
            }
        };
        if let Some((action, pressed)) = reported {
            self.report(
                id,
                SurfaceEvent::Pointer {
                    action,
                    event: crate::input::pointer::mouse(at, pressed),
                    modifiers: held,
                    timestamp,
                },
            );
        }
    }

    /// Reports a scroll.
    #[expect(clippy::too_many_arguments, reason = "one event's whole content")]
    fn wheel(
        &mut self,
        id: SurfaceId,
        horizontal: &smithay_client_toolkit::seat::pointer::AxisScroll,
        vertical: &smithay_client_toolkit::seat::pointer::AxisScroll,
        source: Option<smithay_client_toolkit::reexports::client::protocol::wl_pointer::AxisSource>,
        at: zgui_geom::Point<zgui_geom::CssPx, zgui_geom::Css>,
        modifiers: zgui_vocab::Modifiers,
        timestamp: zgui_vocab::Timestamp,
    ) {
        use smithay_client_toolkit::reexports::client::protocol::wl_pointer::AxisSource;

        let continuous = matches!(source, Some(AxisSource::Finger | AxisSource::Continuous));
        let stopping = horizontal.stop || vertical.stop;
        let phase = axis::phase(continuous, stopping, self.gesturing);
        self.gesturing = continuous && !stopping;
        self.report(
            id,
            SurfaceEvent::Wheel {
                event: zgui_vocab::WheelEvent {
                    delta: axis::delta(horizontal, vertical),
                    phase,
                    position: at,
                    id: zgui_vocab::PointerId::MOUSE,
                    kind: if continuous {
                        zgui_vocab::PointerKind::Touch
                    } else {
                        zgui_vocab::PointerKind::Mouse
                    },
                },
                modifiers,
                timestamp,
            },
        );
    }
}

delegate_keyboard!(WaylandState);
delegate_pointer!(WaylandState);
delegate_touch!(WaylandState);
