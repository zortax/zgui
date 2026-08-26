//! Descriptions of the devices libinput is reading.
//!
//! A [`Device`] is a plain value: the node it was opened at, the names and identifiers the kernel
//! publishes, and the [`Capabilities`] libinput assigned it. libinput's own device is
//! reference-counted and every reading of one is a call into the library, so the reference stays
//! inside the [`Context`](crate::Context) and only this description leaves.

use std::path::{Path, PathBuf};

/// Identifies one device within a [`Context`](crate::Context).
///
/// Every event carries one, because the state a caller keeps for a device — which of its keys are
/// down, which of its buttons are held — belongs to that device alone.
///
/// An id is issued when a device arrives and is never issued twice. A
/// [`suspend`](crate::Context::suspend) and the [`resume`](crate::Context::resume) after it
/// therefore give one node two ids, so state kept under the old id cannot be matched against the
/// reopened device by mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(u64);

impl DeviceId {
    /// Creates the id with the given number.
    ///
    /// A [`Context`](crate::Context) issues its own ids. This is for building an
    /// [`Event`](crate::Event) directly, which is how the code above this crate is tested without
    /// a device.
    #[must_use]
    pub const fn new(number: u64) -> Self {
        Self(number)
    }

    /// Returns the number this id was created with.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A class of input libinput assigns to a device.
///
/// A device may have several. One node that carries a key map, two relative axes and a wheel is
/// both a [`Keyboard`](Capability::Keyboard) and a [`Pointer`](Capability::Pointer); wireless
/// receivers commonly present exactly that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// Keys, reported as [`Event::Key`](crate::Event::Key).
    Keyboard,
    /// Pointer motion, buttons and scrolling. Touchpads are pointers: libinput turns contacts on
    /// the pad into motion.
    Pointer,
    /// Contacts reported by position. This crate does not read them.
    Touch,
    /// A stylus or another tool on a graphics tablet. This crate does not read them.
    TabletTool,
    /// The buttons and rings on a graphics tablet body. This crate does not read them.
    TabletPad,
    /// Swipe, pinch and hold gestures. This crate does not read them.
    Gesture,
    /// A lid or tablet-mode switch. This crate does not read them.
    Switch,
}

impl Capability {
    /// Every capability, in the order libinput numbers them.
    // The order is the interface's: an entry's position is the number
    // `libinput_device_has_capability` is asked with.
    pub(crate) const EVERY: [Self; 7] = [
        Self::Keyboard,
        Self::Pointer,
        Self::Touch,
        Self::TabletTool,
        Self::TabletPad,
        Self::Gesture,
        Self::Switch,
    ];

    /// Returns the number libinput knows this capability by.
    pub(crate) const fn as_raw(self) -> u32 {
        match self {
            Self::Keyboard => 0,
            Self::Pointer => 1,
            Self::Touch => 2,
            Self::TabletTool => 3,
            Self::TabletPad => 4,
            Self::Gesture => 5,
            Self::Switch => 6,
        }
    }

    /// Returns the bit this capability occupies in a [`Capabilities`].
    const fn bit(self) -> u8 {
        1 << self.as_raw()
    }
}

/// The set of [`Capability`] values a device has.
///
/// ```
/// use zgui_libinput::{Capabilities, Capability};
///
/// let receiver = Capabilities::NONE
///     .with(Capability::Keyboard)
///     .with(Capability::Pointer);
///
/// assert!(receiver.keyboard());
/// assert!(receiver.pointer());
/// assert!(!receiver.has(Capability::Touch));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities(u8);

impl Capabilities {
    /// The empty set.
    pub const NONE: Self = Self(0);

    /// Returns this set with `capability` added.
    #[must_use]
    pub const fn with(self, capability: Capability) -> Self {
        Self(self.0 | capability.bit())
    }

    /// Returns `true` if the set contains `capability`.
    #[must_use]
    pub const fn has(self, capability: Capability) -> bool {
        self.0 & capability.bit() != 0
    }

    /// Returns `true` if the device reports keys.
    #[must_use]
    pub const fn keyboard(self) -> bool {
        self.has(Capability::Keyboard)
    }

    /// Returns `true` if the device reports pointer motion, buttons or scrolling.
    #[must_use]
    pub const fn pointer(self) -> bool {
        self.has(Capability::Pointer)
    }
}

/// A device libinput is reading, as it described it when it arrived.
///
/// Carried by [`Event::DeviceAdded`](crate::Event::DeviceAdded) and
/// [`Event::DeviceRemoved`](crate::Event::DeviceRemoved), and returned by
/// [`Context::device`](crate::Context::device).
///
/// ```
/// use zgui_libinput::{Capabilities, Capability, Device, DeviceId};
///
/// let keyboard = Device::new(
///     DeviceId::new(0),
///     "/dev/input/event4",
///     "Example Keyboard",
///     "event4",
///     0x1532,
///     0x0271,
///     Capabilities::NONE.with(Capability::Keyboard),
/// );
///
/// assert_eq!(keyboard.sysname(), "event4");
/// assert!(keyboard.capabilities().keyboard());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Identifies this device within its context.
    id: DeviceId,
    /// The node it was opened at.
    path: PathBuf,
    /// The name the kernel publishes, such as `Razer Razer BlackWidow V3 Tenkeyless`.
    name: String,
    /// The name in `/sys/class/input`, such as `event4`.
    sysname: String,
    /// The vendor identifier.
    vendor: u32,
    /// The product identifier.
    product: u32,
    /// The classes of input libinput assigned it.
    capabilities: Capabilities,
}

impl Device {
    /// Creates a description with the given values.
    ///
    /// A [`Context`](crate::Context) builds these from what libinput reports. This is public so
    /// that code above the crate can be tested without a device.
    #[must_use]
    pub fn new(
        id: DeviceId,
        path: impl Into<PathBuf>,
        name: impl Into<String>,
        sysname: impl Into<String>,
        vendor: u32,
        product: u32,
        capabilities: Capabilities,
    ) -> Self {
        Self {
            id,
            path: path.into(),
            name: name.into(),
            sysname: sysname.into(),
            vendor,
            product,
            capabilities,
        }
    }

    /// Returns the id that identifies this device within its context.
    #[must_use]
    pub const fn id(&self) -> DeviceId {
        self.id
    }

    /// Returns the node the device was opened at.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the name the kernel publishes for the device.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the name of the device in `/sys/class/input`, such as `event4`.
    #[must_use]
    pub fn sysname(&self) -> &str {
        &self.sysname
    }

    /// Returns the vendor identifier.
    #[must_use]
    pub const fn vendor(&self) -> u32 {
        self.vendor
    }

    /// Returns the product identifier.
    #[must_use]
    pub const fn product(&self) -> u32 {
        self.product
    }

    /// Returns the classes of input libinput assigned to the device.
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
}

#[cfg(test)]
mod tests {
    //! The two questions a description answers.

    use super::*;

    #[test]
    fn a_device_can_be_two_things_at_once() {
        // The shape a wireless receiver has: one node carrying a key map, two relative axes and a
        // wheel. Reading it as one or the other would lose half of what somebody does with it.
        let both = Capabilities::NONE
            .with(Capability::Keyboard)
            .with(Capability::Pointer);

        assert!(both.keyboard());
        assert!(both.pointer());
        assert!(!both.has(Capability::Touch));
    }

    #[test]
    fn every_capability_has_its_own_bit() {
        // A `bit` written as a shift by the wrong number would make two capabilities one, and a
        // touchpad would answer that it is a tablet.
        for capability in Capability::EVERY {
            let only = Capabilities::NONE.with(capability);
            for other in Capability::EVERY {
                assert_eq!(
                    only.has(other),
                    other == capability,
                    "{capability:?} answers for {other:?}"
                );
            }
        }
    }

    #[test]
    fn the_capability_numbers_are_the_ones_the_header_gives() {
        // Written out rather than derived. These cross into `libinput_device_has_capability`, and a
        // wrong one would ask about a different capability and be believed.
        assert_eq!(Capability::Keyboard.as_raw(), 0);
        assert_eq!(Capability::Pointer.as_raw(), 1);
        assert_eq!(Capability::Touch.as_raw(), 2);
        assert_eq!(Capability::TabletTool.as_raw(), 3);
        assert_eq!(Capability::TabletPad.as_raw(), 4);
        assert_eq!(Capability::Gesture.as_raw(), 5);
        assert_eq!(Capability::Switch.as_raw(), 6);
    }

    #[test]
    fn a_device_reports_what_it_was_built_with() {
        let device = Device::new(
            DeviceId::new(3),
            "/dev/input/event4",
            "a keyboard",
            "event4",
            0x1532,
            0x0271,
            Capabilities::NONE.with(Capability::Keyboard),
        );

        assert_eq!(device.id(), DeviceId::new(3));
        assert_eq!(device.path(), Path::new("/dev/input/event4"));
        assert_eq!(device.name(), "a keyboard");
        assert_eq!(device.sysname(), "event4");
        assert_eq!(device.vendor(), 0x1532);
        assert_eq!(device.product(), 0x0271);
        assert!(device.capabilities().keyboard());
    }
}
