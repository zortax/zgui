//! Which serial a request may quote.

use wayland_client::protocol::wl_seat::WlSeat;

/// The last input serials this seat produced, kept for the requests that need one.
///
/// A compositor grants a pop-up grab, an interactive move or a resize only against a serial from
/// an event the user actually caused, and only against a **press**. Quoting a release serial, or
/// one from a motion, is declined — for a pop-up that means it is dismissed the instant it opens,
/// which looks like a menu that will not stay open rather than like a refused request.
///
/// So presses are what is recorded, and the latest one is what every such request quotes.
#[derive(Debug, Default, Clone)]
pub struct Serials {
    /// The seat these belong to, once one has produced anything.
    pub(crate) seat: Option<WlSeat>,
    /// The latest serial from a press, of either kind.
    press: Option<u32>,
    /// The latest serial from anything at all, for the requests that take any.
    latest: Option<u32>,
}

impl Serials {
    /// Records a serial from a press.
    pub fn pressed(&mut self, serial: u32) {
        self.press = Some(serial);
        self.latest = Some(serial);
    }

    /// Records a serial from something that was not a press.
    pub fn observed(&mut self, serial: u32) {
        self.latest = Some(serial);
    }

    /// The serial a grab, a move or a resize may quote.
    ///
    /// Absent before the user has pressed anything, which is exactly when such a request would be
    /// refused anyway.
    pub const fn grab(&self) -> Option<u32> {
        self.press
    }

    /// The serial a request that takes any recent one may quote.
    pub const fn any(&self) -> Option<u32> {
        self.latest
    }
}

#[cfg(test)]
mod tests {
    use super::Serials;

    #[test]
    fn nothing_can_be_quoted_before_anything_happened() {
        let serials = Serials::default();
        assert_eq!(serials.grab(), None);
        assert_eq!(serials.any(), None);
    }

    #[test]
    fn a_release_does_not_become_something_a_grab_can_quote() {
        // Quoting a release serial is how a menu is dismissed the instant it opens.
        let mut serials = Serials::default();
        serials.pressed(7);
        serials.observed(8);
        assert_eq!(serials.grab(), Some(7));
        assert_eq!(serials.any(), Some(8));
    }

    #[test]
    fn the_latest_press_is_the_one_that_counts() {
        let mut serials = Serials::default();
        serials.pressed(1);
        serials.pressed(2);
        assert_eq!(serials.grab(), Some(2));
    }
}
