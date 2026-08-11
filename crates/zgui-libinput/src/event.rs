//! What happened, as a value.
//!
//! libinput's own event is valid until it is destroyed, and every reader of one is a call into the
//! library. Each event is read into one of these at the edge, so everything above this crate works
//! on values it can also write.

use std::time::Duration;

use crate::device::{Device, DeviceId};

/// Which way a key or a button moved.
///
/// ```
/// use zgui_libinput::Press;
///
/// // What a caller keeps for one device: the keys that are down.
/// let mut held: Vec<u32> = Vec::new();
/// for (key, press) in [(30, Press::Down), (48, Press::Down), (30, Press::Up)] {
///     match press {
///         Press::Down => held.push(key),
///         Press::Up => held.retain(|down| *down != key),
///     }
/// }
///
/// assert_eq!(held, [48]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// It went down.
    Down,
    /// It came up.
    Up,
}

impl Press {
    /// Returns what libinput's `0` and `1` mean.
    ///
    /// Every other number is read as released. A key reported down and never up is one held for
    /// the rest of the run.
    pub(crate) const fn of(state: u32) -> Self {
        match state {
            1 => Self::Down,
            _ => Self::Up,
        }
    }
}

/// Where a scroll came from.
///
/// The source names the unit the numbers are in: a wheel counts in one hundred and twentieths of a
/// detent, so one whole detent is `120.0`, and the other two sources count in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scrolled {
    /// A wheel, in one hundred and twentieths of a detent.
    ///
    /// A free-spinning wheel reports fine movement continuously, and a whole detent only when it
    /// has accumulated one. The fine unit is therefore the one that is read.
    Wheel,
    /// Two fingers on a touchpad, in pixels.
    ///
    /// A value of zero on an axis is this source reporting that the fingers stopped. That is what
    /// ends a kinetic scroll.
    Finger,
    /// A device that scrolls by being pushed, such as a trackpoint under a held button, in pixels.
    Continuous,
}

/// One thing a device did.
///
/// ```
/// use std::time::Duration;
/// use zgui_libinput::{Capabilities, Device, DeviceId, Event, Press};
///
/// let key = Event::Key {
///     device: DeviceId::new(0),
///     key: 30,
///     press: Press::Down,
///     at: Duration::from_micros(1_000),
/// };
///
/// assert_eq!(key.device(), DeviceId::new(0));
/// assert_eq!(key.at(), Some(Duration::from_micros(1_000)));
///
/// // A device arriving names the device it carries, and carries no time.
/// let arrived = Event::DeviceAdded(Device::new(
///     DeviceId::new(1),
///     "/dev/input/event4",
///     "Example Keyboard",
///     "event4",
///     0x1532,
///     0x0271,
///     Capabilities::NONE,
/// ));
///
/// assert_eq!(arrived.device(), DeviceId::new(1));
/// assert_eq!(arrived.at(), None);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A device is now being read.
    ///
    /// This arrives for a device that was just added, and for every device again after a resume.
    /// The description is carried rather than looked up, because a caller wants it exactly here.
    DeviceAdded(Device),
    /// A device is no longer being read.
    ///
    /// Three things produce it: a device that was removed, a device that stopped answering, and a
    /// suspend, which removes all of them. What a caller held for that device is now stale — which
    /// keys it had down, which buttons it was holding — and this is where it is dropped.
    DeviceRemoved(Device),
    /// Somebody moved a key.
    Key {
        /// Which device it was struck on. One layout serves every keyboard, and which keys are down
        /// belongs to the device.
        device: DeviceId,
        /// The kernel's own code for the key. A layout is asked about this code.
        key: u32,
        /// Which way it moved.
        ///
        /// libinput reports no repeats: how long a key is held before it repeats, and how often, is
        /// a decision about a person rather than about a device.
        press: Press,
        /// When it moved.
        at: Duration,
    },
    /// A pointing device said how far it moved.
    ///
    /// The numbers are **after** libinput's acceleration. A slow drag and a fast flick over the
    /// same distance therefore move a pointer by different amounts.
    Motion {
        /// Which device moved.
        device: DeviceId,
        /// How far across, in pixels.
        dx: f64,
        /// How far down, in pixels.
        dy: f64,
        /// When it moved.
        at: Duration,
    },
    /// A pointing device said where it is.
    ///
    /// A touchscreen and a graphics tablet report this way. Each number is a fraction of that axis
    /// of the device, so what it means on a screen is the caller's answer.
    MotionAbsolute {
        /// Which device reported.
        device: DeviceId,
        /// Where across, from `0.0` to `1.0`.
        x: f64,
        /// Where down, from `0.0` to `1.0`.
        y: f64,
        /// When it reported.
        at: Duration,
    },
    /// Somebody moved a button.
    Button {
        /// Which device it was pressed on. Which buttons are down belongs to the device, for the
        /// reason keys do.
        device: DeviceId,
        /// The kernel's own code for the button.
        button: u32,
        /// Which way it moved.
        press: Press,
        /// When it moved.
        at: Duration,
    },
    /// Somebody scrolled.
    ///
    /// An axis that is absent is different from one that scrolled by nothing: a finger source
    /// reports zero to say that the fingers stopped.
    Scroll {
        /// Which device scrolled.
        device: DeviceId,
        /// What it was scrolled with. The source names the unit the two numbers are in.
        source: Scrolled,
        /// How far down the scroll went, where this scroll carries the vertical axis.
        vertical: Option<f64>,
        /// How far right the scroll went, where this scroll carries the horizontal axis.
        horizontal: Option<f64>,
        /// When it happened.
        at: Duration,
    },
}

impl Event {
    /// Returns the device this event is about.
    #[must_use]
    pub const fn device(&self) -> DeviceId {
        match self {
            Self::DeviceAdded(device) | Self::DeviceRemoved(device) => device.id(),
            Self::Key { device, .. }
            | Self::Motion { device, .. }
            | Self::MotionAbsolute { device, .. }
            | Self::Button { device, .. }
            | Self::Scroll { device, .. } => *device,
        }
    }

    /// Returns when it happened, for the events that carry a time.
    ///
    /// A device arriving and a device going carry none: neither is something a device did.
    #[must_use]
    pub const fn at(&self) -> Option<Duration> {
        match self {
            Self::DeviceAdded(_) | Self::DeviceRemoved(_) => None,
            Self::Key { at, .. }
            | Self::Motion { at, .. }
            | Self::MotionAbsolute { at, .. }
            | Self::Button { at, .. }
            | Self::Scroll { at, .. } => Some(*at),
        }
    }
}

#[cfg(test)]
mod tests {
    //! What an event answers about itself.

    use super::*;
    use crate::device::Capabilities;

    #[test]
    fn a_press_is_one_and_everything_else_is_a_release() {
        // The numbers cross from libinput as a C enum. A state this crate does not know is read as
        // a release, because a key reported down and never up is one held for the rest of the run.
        assert_eq!(Press::of(1), Press::Down);
        assert_eq!(Press::of(0), Press::Up);
        assert_eq!(Press::of(7), Press::Up);
    }

    #[test]
    fn every_event_names_the_device_it_came_from() {
        // Which keys are down and which buttons are held belong to the device rather than to the
        // seat, so an event that could not name its device would be one nothing can file.
        let device = DeviceId::new(4);
        let at = Duration::from_micros(1_000);

        let events = [
            Event::Key {
                device,
                key: 30,
                press: Press::Down,
                at,
            },
            Event::Motion {
                device,
                dx: 1.0,
                dy: -2.0,
                at,
            },
            Event::MotionAbsolute {
                device,
                x: 0.5,
                y: 0.25,
                at,
            },
            Event::Button {
                device,
                button: 0x110,
                press: Press::Up,
                at,
            },
            Event::Scroll {
                device,
                source: Scrolled::Wheel,
                vertical: Some(120.0),
                horizontal: None,
                at,
            },
        ];

        for event in events {
            assert_eq!(event.device(), device, "{event:?}");
            assert_eq!(event.at(), Some(at), "{event:?}");
        }
    }

    #[test]
    fn a_device_arriving_is_about_that_device_and_has_no_time() {
        let device = Device::new(
            DeviceId::new(2),
            "/dev/input/event4",
            "a keyboard",
            "event4",
            0,
            0,
            Capabilities::NONE,
        );

        let arrived = Event::DeviceAdded(device.clone());
        assert_eq!(arrived.device(), device.id());
        assert_eq!(
            arrived.at(),
            None,
            "a device arriving is not something a device did"
        );
    }
}
