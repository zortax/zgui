//! The token of an input event, for a request the desktop grants only against one.

use core::fmt::{self, Debug};

/// Which press of the user a request answers.
///
/// Some desktops grant a request only when it quotes an input event the user caused: a pop-up
/// that takes the pointer and the keyboard, an interactive move, a claim on a selection. This is
/// the opaque token a backend gives such an event. Read it with
/// [`Surface::input_serial`](crate::Surface::input_serial) in the handler of the press that opens
/// the pop-up, and pass it on with the request.
///
/// A desktop that needs no token never gives one, and a request that carries one there ignores
/// it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputSerial(u32);

impl InputSerial {
    /// The token with the given raw value.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw value, for the backend that gave it.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl Debug for InputSerial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "InputSerial({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::InputSerial;

    #[test]
    fn a_serial_round_trips_and_compares() {
        assert_eq!(InputSerial::new(41).raw(), 41);
        assert_ne!(InputSerial::new(1), InputSerial::new(2));
        assert_eq!(format!("{:?}", InputSerial::new(3)), "InputSerial(3)");
    }
}
