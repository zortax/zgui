//! One device the seat opened.

use std::ffi::c_int;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

/// A device opened on a seat.
///
/// The seat opens the device and hands the descriptor over, so a program that asks for one this way
/// needs no privilege of its own.
///
/// # Ids and descriptors
///
/// libseat answers an id for every device it opens and writes the descriptor beside it. The logind
/// and noop backends answer the descriptor's own number as the id, and the seatd backend answers an
/// id of its own, so the two are held apart here. [`Device::id`] gives the device back, and
/// [`Device::descriptor`] is what the device is used through.
///
/// # Ownership of the descriptor
///
/// libseat closes no descriptor. `libseat_close_device` releases the device with the session
/// daemon, `libseat_close_seat` releases the seat, and both leave every descriptor open. So a
/// `Device` owns the descriptor libseat wrote, and dropping one closes it.
///
/// # Giving a device back
///
/// [`crate::Seat::close_device`] tells the session daemon that the device is free and then closes
/// the descriptor. Dropping a `Device` closes the descriptor alone, and the daemon keeps its record
/// of the device until the seat closes. A program that opens its devices again on every terminal
/// switch leaves one such record per switch behind, so it gives each device back.
#[derive(Debug)]
pub struct Device {
    /// libseat's id for this device, the number that gives it back.
    id: c_int,
    /// The descriptor libseat opened for this device, owned here.
    descriptor: OwnedFd,
}

impl Device {
    /// The device libseat answered, over the id and the descriptor it wrote.
    pub(crate) fn new(id: c_int, descriptor: OwnedFd) -> Self {
        Self { id, descriptor }
    }

    /// libseat's id for this device.
    ///
    /// This is the number `libseat_close_device` takes, and it is libseat's own. A caller that needs
    /// the descriptor asks [`Device::descriptor`] for it.
    pub fn id(&self) -> c_int {
        self.id
    }

    /// Returns the descriptor the device is used through.
    ///
    /// It is open until the device is dropped or given back. The borrow is tied to `&self`, so it
    /// ends before either.
    pub fn descriptor(&self) -> BorrowedFd<'_> {
        self.descriptor.as_fd()
    }
}
