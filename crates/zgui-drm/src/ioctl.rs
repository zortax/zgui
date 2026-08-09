//! The request numbers, and the one call that issues them.
//!
//! A request number is `_IOWR(type, nr, sizeof(struct))`. `rustix::ioctl::opcode` computes that
//! arithmetic as a const function over `size_of::<T>()`, so every number below is derived from the
//! generated struct instead of being transcribed beside it. A struct generated at the wrong size
//! therefore changes its own request number, and the sizes asserted in `sys` are what catch it.

// The table below is the kernel's interface. Every entry is a constant the headers define, and the
// test at the foot of this file checks each one against the value those headers expand to. The
// callers arrive over the tasks that follow, so without this allow, building the table in one piece
// would fail `-D warnings` until the last caller was written.
#![allow(dead_code)]

use std::ffi::c_void;
use std::marker::PhantomData;

use rustix::fd::BorrowedFd;
use rustix::ioctl::{Ioctl, IoctlOutput, Opcode, opcode};

use crate::error::{Error, Result};
use crate::sys;

/// The character every DRM request number is grouped under.
const GROUP: u8 = b'd';

/// One request: what to ask the kernel, the payload the number was computed for, and the name to
/// report when the kernel refuses.
///
/// The type parameter ties the request number to its payload. The kernel writes back as many bytes
/// as the request number encodes, so a request issued with a smaller payload writes past it.
/// `issue` is a safe function, so the compiler holds the pairing.
pub(crate) struct Request<T> {
    /// The computed request number.
    opcode: Opcode,
    /// What this is, for the error message.
    name: &'static str,
    /// Ties the number to the type its size was computed from, and carries no value.
    payload: PhantomData<fn(&mut T)>,
}

impl<T> Clone for Request<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Request<T> {}

impl<T> Request<T> {
    /// Returns the request number, for the tests that check it against the header.
    pub(crate) const fn opcode(self) -> Opcode {
        self.opcode
    }
}

/// Names a request that reads and writes its payload.
macro_rules! read_write {
    ($name:ident, $number:expr, $payload:ty) => {
        pub(crate) const $name: Request<$payload> = Request {
            opcode: opcode::read_write::<$payload>(GROUP, $number),
            name: stringify!($name),
            payload: PhantomData,
        };
    };
}

/// Names a request that only writes its payload to the kernel.
macro_rules! write_only {
    ($name:ident, $number:expr, $payload:ty) => {
        pub(crate) const $name: Request<$payload> = Request {
            opcode: opcode::write::<$payload>(GROUP, $number),
            name: stringify!($name),
            payload: PhantomData,
        };
    };
}

/// Names a request that carries no payload.
///
/// The kernel reads and writes nothing through a `DRM_IO` number, and `()` says so.
macro_rules! no_payload {
    ($name:ident, $number:expr) => {
        pub(crate) const $name: Request<()> = Request {
            opcode: opcode::none(GROUP, $number),
            name: stringify!($name),
            payload: PhantomData,
        };
    };
}

write_only!(GEM_CLOSE, 0x09, sys::drm_gem_close);
read_write!(GET_CAP, 0x0c, sys::drm_get_cap);
write_only!(SET_CLIENT_CAP, 0x0d, sys::drm_set_client_cap);
no_payload!(SET_MASTER, 0x1e);
no_payload!(DROP_MASTER, 0x1f);
read_write!(PRIME_HANDLE_TO_FD, 0x2d, sys::drm_prime_handle);
read_write!(PRIME_FD_TO_HANDLE, 0x2e, sys::drm_prime_handle);
read_write!(MODE_GETRESOURCES, 0xa0, sys::drm_mode_card_res);
read_write!(MODE_SETCRTC, 0xa2, sys::drm_mode_crtc);
read_write!(MODE_GETENCODER, 0xa6, sys::drm_mode_get_encoder);
read_write!(MODE_GETCONNECTOR, 0xa7, sys::drm_mode_get_connector);
read_write!(MODE_GETPROPERTY, 0xaa, sys::drm_mode_get_property);
read_write!(MODE_GETPROPBLOB, 0xac, sys::drm_mode_get_blob);
read_write!(MODE_RMFB, 0xaf, u32);
read_write!(MODE_PAGE_FLIP, 0xb0, sys::drm_mode_crtc_page_flip);
read_write!(MODE_CREATE_DUMB, 0xb2, sys::drm_mode_create_dumb);
read_write!(MODE_MAP_DUMB, 0xb3, sys::drm_mode_map_dumb);
read_write!(MODE_DESTROY_DUMB, 0xb4, sys::drm_mode_destroy_dumb);
read_write!(MODE_GETPLANERESOURCES, 0xb5, sys::drm_mode_get_plane_res);
read_write!(MODE_GETPLANE, 0xb6, sys::drm_mode_get_plane);
read_write!(MODE_ADDFB2, 0xb8, sys::drm_mode_fb_cmd2);
read_write!(
    MODE_OBJ_GETPROPERTIES,
    0xb9,
    sys::drm_mode_obj_get_properties
);
read_write!(MODE_CURSOR2, 0xbb, sys::drm_mode_cursor2);
read_write!(MODE_ATOMIC, 0xbc, sys::drm_mode_atomic);
read_write!(MODE_CREATEPROPBLOB, 0xbd, sys::drm_mode_create_blob);
read_write!(MODE_DESTROYPROPBLOB, 0xbe, sys::drm_mode_destroy_blob);

/// One ioctl, with its payload borrowed for the duration of the call.
struct Call<'a, T> {
    /// The request number.
    opcode: Opcode,
    /// What the kernel reads and writes.
    payload: &'a mut T,
}

// SAFETY: `as_ptr` hands back a pointer to `payload`, which is a live `&mut T` for the whole call,
// so it is valid, aligned and uniquely borrowed. `issue` accepts a `Request<T>` only with a
// payload of that same `T`, and a request number is computed from the `T` it is typed for, so the
// size the kernel reads out of the number and the size of what it is pointed at agree by
// construction. `IS_MUTATING` is true because the read-write and write requests here all have the
// kernel write back into the payload.
unsafe impl<T> Ioctl for Call<'_, T> {
    type Output = ();

    const IS_MUTATING: bool = true;

    fn opcode(&self) -> Opcode {
        self.opcode
    }

    fn as_ptr(&mut self) -> *mut c_void {
        std::ptr::from_mut(self.payload).cast()
    }

    unsafe fn output_from_ptr(_: IoctlOutput, _: *mut c_void) -> rustix::io::Result<()> {
        Ok(())
    }
}

/// Issues `request` against `fd`, with `payload` as its argument.
///
/// A DRM ioctl is restarted on `EINTR` and on `EAGAIN`. The kernel returns the first when a signal
/// arrives mid-call and the second when the device is busy, and neither answer means the request
/// failed. libdrm's own `drmIoctl` loops on exactly these two.
pub(crate) fn issue<T>(fd: BorrowedFd<'_>, request: Request<T>, payload: &mut T) -> Result<()> {
    loop {
        // SAFETY: the claims are stated on the `Ioctl` implementation above. `fd` is a live
        // borrowed descriptor for the duration of the call.
        let outcome = unsafe {
            rustix::ioctl::ioctl(
                fd,
                Call {
                    opcode: request.opcode(),
                    payload,
                },
            )
        };
        return match outcome {
            Ok(()) => Ok(()),
            Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => continue,
            Err(errno) => Err(Error::Ioctl {
                request: request.name,
                source: errno.into(),
            }),
        };
    }
}

#[cfg(test)]
mod tests {
    //! The request numbers, against the values the headers produce.

    use super::*;

    #[test]
    fn the_request_numbers_are_the_ones_the_headers_expand_to() {
        // Each number is what `DRM_IOWR(0xa0, struct drm_mode_card_res)` and its neighbours expand
        // to when the C preprocessor is run over the kernel's own headers. A struct generated at
        // the wrong size changes the number, and this is where that shows up as a failure instead
        // of as `EINVAL` from a device.
        assert_eq!(GEM_CLOSE.opcode(), 0x4008_6409);
        assert_eq!(GET_CAP.opcode(), 0xc010_640c);
        assert_eq!(SET_CLIENT_CAP.opcode(), 0x4010_640d);
        assert_eq!(SET_MASTER.opcode(), 0x0000_641e);
        assert_eq!(DROP_MASTER.opcode(), 0x0000_641f);
        assert_eq!(PRIME_HANDLE_TO_FD.opcode(), 0xc00c_642d);
        assert_eq!(PRIME_FD_TO_HANDLE.opcode(), 0xc00c_642e);
        assert_eq!(MODE_GETRESOURCES.opcode(), 0xc040_64a0);
        assert_eq!(MODE_SETCRTC.opcode(), 0xc068_64a2);
        assert_eq!(MODE_GETENCODER.opcode(), 0xc014_64a6);
        assert_eq!(MODE_GETCONNECTOR.opcode(), 0xc050_64a7);
        assert_eq!(MODE_GETPROPERTY.opcode(), 0xc040_64aa);
        assert_eq!(MODE_GETPROPBLOB.opcode(), 0xc010_64ac);
        assert_eq!(MODE_RMFB.opcode(), 0xc004_64af);
        assert_eq!(MODE_PAGE_FLIP.opcode(), 0xc018_64b0);
        assert_eq!(MODE_CREATE_DUMB.opcode(), 0xc020_64b2);
        assert_eq!(MODE_MAP_DUMB.opcode(), 0xc010_64b3);
        assert_eq!(MODE_DESTROY_DUMB.opcode(), 0xc004_64b4);
        assert_eq!(MODE_GETPLANERESOURCES.opcode(), 0xc010_64b5);
        assert_eq!(MODE_GETPLANE.opcode(), 0xc020_64b6);
        assert_eq!(MODE_ADDFB2.opcode(), 0xc068_64b8);
        assert_eq!(MODE_OBJ_GETPROPERTIES.opcode(), 0xc020_64b9);
        assert_eq!(MODE_CURSOR2.opcode(), 0xc024_64bb);
        assert_eq!(MODE_ATOMIC.opcode(), 0xc038_64bc);
        assert_eq!(MODE_CREATEPROPBLOB.opcode(), 0xc010_64bd);
        assert_eq!(MODE_DESTROYPROPBLOB.opcode(), 0xc004_64be);
    }

    /// Compiles only if `request` may be issued with a payload of type `T`.
    fn assert_payload<T>(_: Request<T>) {}

    #[test]
    fn a_request_carries_the_payload_type_its_number_was_computed_for() {
        // What this refuses matters more than what it accepts:
        // `assert_payload::<sys::drm_get_cap>(MODE_ATOMIC)` does not compile, so a call site cannot
        // pair a number with a payload smaller than the bytes the kernel writes back through it.
        assert_payload::<sys::drm_get_cap>(GET_CAP);
        assert_payload::<sys::drm_mode_atomic>(MODE_ATOMIC);
        assert_payload::<()>(SET_MASTER);
    }
}
