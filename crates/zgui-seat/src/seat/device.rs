//! One device the seat opened.

use std::ffi::c_int;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};

/// Which seat a device came from.
///
/// A device id belongs to the seat that answered it. Another seat numbers its own devices, so one
/// given a device it never opened releases one of its own, or none. Every seat takes the next
/// number here and every device carries the number of the seat that opened it, so the two are
/// compared before libseat is asked.
///
/// A number is never handed out twice, so a seat that has closed cannot be taken for one that
/// opened after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Token(u64);

impl Token {
    /// The number for one more seat.
    pub(crate) fn next() -> Self {
        /// How many seats this process has opened.
        static OPENED: AtomicU64 = AtomicU64::new(0);

        Self(OPENED.fetch_add(1, Ordering::Relaxed))
    }
}

/// A device opened on a seat.
///
/// The seat opens the device and hands the descriptor over, so a program that asks for one this way
/// needs no privilege of its own.
///
/// ```no_run
/// use std::path::Path;
/// use zgui_seat::Seat;
///
/// let seat = Seat::open()?;
/// let card = seat.open_device(Path::new("/dev/dri/card0"))?;
///
/// println!("the card arrived as {:?}", card.descriptor());
/// seat.close_device(card)?;
/// # Ok::<(), zgui_seat::Error>(())
/// ```
///
/// # Ids and descriptors
///
/// libseat answers an id for every device it opens and writes the descriptor beside it. The logind
/// and noop backends answer the descriptor's own number as the id, and the seatd backend answers an
/// id of its own, so the two are held apart here. The id gives the device back and stays inside
/// this crate, and [`Device::descriptor`] is what the device is used through.
///
/// # Ownership of the descriptor
///
/// libseat closes no descriptor. `libseat_close_device` releases the device with the session daemon,
/// `libseat_close_seat` releases the seat, and both leave every descriptor open. So a `Device` owns
/// the descriptor libseat wrote, and dropping one closes it.
///
/// # Giving a device back
///
/// [`crate::Seat::close_device`] tells the session daemon that the device is free and then closes
/// the descriptor. It is the seat that opened the device: a device carries which one that was, and
/// any other seat refuses it. Dropping a `Device` closes the descriptor alone, and the daemon keeps
/// its record of the device until the seat closes. A program that opens its devices again on every
/// terminal switch leaves one such record per switch behind, so it gives each device back.
#[derive(Debug)]
pub struct Device {
    /// The seat that opened this device, which is the one that can give it back.
    seat: Token,
    /// libseat's id for this device, the number that gives it back.
    id: c_int,
    /// The descriptor libseat opened for this device, owned here.
    descriptor: OwnedFd,
}

impl Device {
    /// The device libseat answered, over the id and the descriptor it wrote.
    pub(crate) fn new(seat: Token, id: c_int, descriptor: OwnedFd) -> Self {
        Self {
            seat,
            id,
            descriptor,
        }
    }

    /// Returns the seat that opened this device.
    pub(crate) fn seat(&self) -> Token {
        self.seat
    }

    /// Returns libseat's id for this device.
    ///
    /// This is the number `libseat_close_device` takes, and it is libseat's own. It stays inside
    /// this crate: an id is a `c_int` and copies, so a caller holding one could ask libseat for the
    /// device after this value dropped the descriptor, and that is the order the session daemon
    /// reads the wrong device from.
    pub(crate) fn id(&self) -> c_int {
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
