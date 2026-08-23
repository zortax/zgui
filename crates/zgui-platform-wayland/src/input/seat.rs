//! What is attached to a seat, and where its input goes.

use smithay_client_toolkit::reexports::client::protocol::wl_keyboard::WlKeyboard;
use smithay_client_toolkit::reexports::client::protocol::wl_touch::WlTouch;
use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceId;
use zgui_vocab::Modifiers;

use smithay_client_toolkit::reexports::client::protocol::wl_seat::WlSeat;

use crate::input::Serials;

/// What a seat turned out to have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Devices {
    /// Whether it can type.
    pub keyboard: bool,
    /// Whether it can point.
    pub pointer: bool,
    /// Whether it can be touched.
    pub touch: bool,
}

/// What taking a seat turned out to mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adopted {
    /// It has just been taken, and everything that belongs to a seat opens now.
    Now,
    /// It is the one already being tracked.
    Already,
    /// Another seat is being tracked and this one is left alone.
    Another,
}

impl Adopted {
    /// Whether this seat is the one this backend speaks for.
    pub const fn is_ours(self) -> bool {
        matches!(self, Self::Now | Self::Already)
    }
}

/// One seat's devices, and what they are pointed at.
///
/// A seat is one person's set of input devices, and a compositor may have several. This backend
/// tracks one, which is what every desktop toolkit does and what every desktop actually has;
/// tracking more is not a matter of a second field but of a second focus, a second held modifier
/// set and a second pointer position everywhere they are read.
#[derive(Default)]
pub struct Seat {
    /// The keyboard, while the seat has one.
    pub(crate) keyboard: Option<WlKeyboard>,
    /// Whether the seat has a pointer.
    ///
    /// The pointer object itself is owned by the link a surface speaks to, because that is what
    /// makes every request against it and a themed pointer cannot be copied.
    pub(crate) pointing: bool,
    /// What the compositor said this seat has, against what has actually been opened.
    ///
    /// Kept because the two are allowed to differ only one way — a device may fail to open — and
    /// any other difference is a defect that produces an application nothing can be typed into.
    /// There is no other way to notice it: every part of the input path stays silent.
    pub(crate) offered: Devices,
    /// The touch device, while the seat has one.
    pub(crate) touch: Option<WlTouch>,
    /// The serials a request may quote.
    pub(crate) serials: Serials,
    /// Which modifiers are held.
    ///
    /// A level rather than an event: the compositor reports a change, and everything above the
    /// contract is written against the set that was held when something happened.
    pub(crate) held: Modifiers,
    /// Which surface has the keyboard.
    pub(crate) keyboard_focus: Option<SurfaceId>,
    /// Which surface the pointer is over.
    pub(crate) pointer_focus: Option<SurfaceId>,
    /// Where the pointer last was, in the space a layout is written in.
    ///
    /// Kept because half the events that need a position do not carry one: a wheel event, a button
    /// release after the pointer left, and a drop all describe something that happened *at* the
    /// pointer without saying where that is.
    pub(crate) at: Point<CssPx, Css>,
}

impl Seat {
    /// A seat with nothing attached and nothing focused.
    pub fn new() -> Self {
        Self {
            at: Point::new(CssPx(0.0), CssPx(0.0)),
            ..Self::default()
        }
    }

    /// Takes `seat` as the one this backend tracks, answering what that means.
    ///
    /// Called wherever a seat is first seen rather than only where the toolkit announces one. The
    /// toolkit announces a seat that is *plugged in*, and says nothing about the seats that already
    /// existed when the program started — which on every ordinary desktop is the only seat there
    /// is. A backend that waited to be told would open no keyboard and no pointer, ever.
    pub fn adopt(&mut self, seat: &WlSeat) -> Adopted {
        match &self.serials.seat {
            Some(held) if held == seat => Adopted::Already,
            // One seat is tracked, which is what every desktop has. A second would need a second
            // focus, a second held set and a second pointer position everywhere those are read.
            Some(_) => Adopted::Another,
            None => {
                self.serials.seat = Some(seat.clone());
                Adopted::Now
            }
        }
    }

    /// Which devices are open, for the properties that check them against what was offered.
    pub const fn opened(&self) -> Devices {
        Devices {
            keyboard: self.keyboard.is_some(),
            pointer: self.pointing,
            touch: self.touch.is_some(),
        }
    }

    /// Records where the pointer is now.
    pub const fn moved(&mut self, at: Point<CssPx, Css>) {
        self.at = at;
    }

    /// Whether the keyboard focus moved, and to where.
    ///
    /// Answered as an edge because focus is one: a compositor restates it, and a field that
    /// settled its value on every restatement would validate a form the user had not left.
    pub fn focus(&mut self, surface: Option<SurfaceId>) -> Option<(Option<SurfaceId>, SurfaceId)> {
        if self.keyboard_focus == surface {
            return None;
        }
        let left = self.keyboard_focus;
        self.keyboard_focus = surface;
        surface.map(|gained| (left, gained)).or_else(|| {
            // Focus went nowhere: the surface that had it is the only one to tell.
            left.map(|lost| (Some(lost), lost))
        })
    }
}

impl core::fmt::Debug for Seat {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Seat")
            .field("keyboard", &self.keyboard.is_some())
            .field("pointer", &self.pointing)
            .field("touch", &self.touch.is_some())
            .field("keyboard_focus", &self.keyboard_focus)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::Seat;
    use zgui_geom::CssPx;
    use zgui_platform::SurfaceId;

    #[test]
    fn nothing_is_open_on_a_seat_that_has_not_been_seen() {
        assert_eq!(Seat::new().opened(), super::Devices::default());
    }

    #[test]
    fn a_seat_starts_with_nothing_attached_and_the_pointer_at_the_origin() {
        let seat = Seat::new();
        assert!(seat.keyboard.is_none());
        assert!(!seat.pointing);
        assert_eq!(seat.at.x, CssPx(0.0));
        assert_eq!(seat.keyboard_focus, None);
    }

    #[test]
    fn focus_is_reported_on_its_edges_and_never_restated() {
        let mut seat = Seat::new();
        let first = SurfaceId::new(1);
        assert_eq!(seat.focus(Some(first)), Some((None, first)));
        assert_eq!(seat.focus(Some(first)), None, "the same focus twice");
    }

    #[test]
    fn losing_focus_names_the_surface_that_lost_it() {
        // Nothing else can: the compositor's leave event names the surface, and the gain that
        // follows may never come at all.
        let mut seat = Seat::new();
        let held = SurfaceId::new(3);
        seat.focus(Some(held));
        assert_eq!(seat.focus(None), Some((Some(held), held)));
        assert_eq!(seat.focus(None), None);
    }

    #[test]
    fn moving_focus_between_two_surfaces_names_both() {
        let mut seat = Seat::new();
        let first = SurfaceId::new(1);
        let second = SurfaceId::new(2);
        seat.focus(Some(first));
        assert_eq!(seat.focus(Some(second)), Some((Some(first), second)));
    }
}
