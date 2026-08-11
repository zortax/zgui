//! What happened, as a value.
//!
//! libinput's own event is valid until it is destroyed, and every reader of one is a call into the
//! library. Each event is read into one of these at the edge, so everything above this crate works
//! on values it can also write.

use crate::device::{Device, DeviceId};

/// One thing a device did.
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
}

impl Event {
    /// Returns the device this event is about.
    #[must_use]
    pub const fn device(&self) -> DeviceId {
        match self {
            Self::DeviceAdded(device) | Self::DeviceRemoved(device) => device.id(),
        }
    }
}
