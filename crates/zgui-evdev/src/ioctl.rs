//! The request numbers, and the calls that issue them.
//!
//! A request number is `_IOC(direction, group, number, size)`. `rustix::ioctl::opcode` computes
//! that arithmetic as a const function over `size_of::<T>()`, so every entry in the table below is
//! derived from the generated struct rather than transcribed beside it. A struct that changed size
//! would change its own request number, so the sizes asserted in `sys` are the thing worth
//! asserting.
//!
//! # Request numbers computed at run time
//!
//! Not every number here is a constant. `EVIOCGNAME(len)`, `EVIOCGKEY(len)` and
//! `EVIOCGBIT(type, len)` encode the length of a byte buffer the *caller* chose, and `EVIOCGBIT`
//! and `EVIOCGABS` put the event type and the axis into the request number itself.
//!
//! Loosening [`Request<T>`] to carry a length would give up the pairing it exists for, so it is
//! left alone and the byte-buffer case is a separate type, [`ByteRequest`], which carries no length
//! at all. [`issue_bytes`] computes the number from the buffer it is handed, so there is nowhere
//! for a second length to be written down and nothing for the two to disagree about.
//!
//! # One request type per call shape
//!
//! There are three ways the kernel takes an argument here, and each has a request type only its
//! own call can name. [`Request<T>`] goes to [`issue`], which hands over a pointer to a `T`.
//! [`ByteRequest`] goes to [`issue_bytes`], which hands over a buffer. [`ValueRequest`] goes to
//! [`issue_value`], which hands the integer over as the argument and points at nothing.
//!
//! Handing one to the wrong call is then a type error. `EVIOCGRAB` reads its argument *as* the
//! value and branches on whether it is null, so a release written through [`issue`] would pass the
//! address of a local — never null — and grab the device it was meant to give back.

// The table below is the kernel's interface. Every entry is a constant the headers define, every
// entry is checked against the value those headers expand to by the test at the foot of this file,
// and the callers arrive over the tasks that follow. Without the allowance, building the table in
// one piece would fail `-D warnings` until the last caller was written.
#![allow(dead_code)]

use std::ffi::{c_int, c_void};
use std::marker::PhantomData;

use rustix::fd::BorrowedFd;
use rustix::ioctl::{Direction, Ioctl, IoctlOutput, Opcode, opcode};

use crate::error::{Error, Result};
use crate::sys;

/// The character every evdev request number is grouped under.
const GROUP: u8 = b'E';

/// The character every uinput request number is grouped under.
const UINPUT: u8 = b'U';

/// The number `EVIOCGBIT` adds the event type to.
const BIT_BASE: u8 = 0x20;

/// The number `EVIOCGABS` adds the axis to.
const ABS_BASE: u8 = 0x40;

/// The largest payload a request number can carry.
///
/// A request number has fourteen bits for the size. `rustix` masks the size into them rather than
/// refusing, and the kernel then reads a length nobody asked for, so the check has to be here.
const MAX_PAYLOAD: usize = (1 << 14) - 1;

/// A request whose argument is the integer itself.
///
/// `EVIOCGRAB` and the `UI_SET_*BIT` family are `_IOW(…, int)` numbers, so the number encodes four
/// bytes, and the kernel reads the argument *as* those bytes without following it.
///
/// A marker payload on [`Request<T>`] would stop nothing, because `issue<T>` accepts any `T` and
/// would accept the marker too. `EVIOCGRAB` branches on `if (p)`, and the address of a local is
/// never null, so a `release` written through [`issue`] *grabs* the device: a keyboard taken from
/// the session with nothing left holding a handle that could give it back. [`issue_value`] is the
/// only call that names this type, and it points at nothing.
#[derive(Clone, Copy)]
pub(crate) struct ValueRequest {
    /// The computed request number.
    opcode: Opcode,
    /// What this is, for the error message.
    name: &'static str,
}

impl ValueRequest {
    /// Returns the request number, for the tests that check it against the header.
    pub(crate) const fn opcode(self) -> Opcode {
        self.opcode
    }
}

/// A request: what to ask the kernel, the payload it is computed for, and the name to report if
/// it refuses.
///
/// The type parameter ties the request number to the payload it was computed from. The kernel
/// writes back the number of bytes the request number encodes, so a request paired with a smaller
/// payload writes past it — and [`issue`] is a safe function, so the compiler holds the pairing.
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

    /// Builds a request that reads `T` back, with a number chosen at run time.
    ///
    /// `EVIOCGABS(axis)` is the reason this exists: the axis is added to the request number, so the
    /// number varies with the call. The size still comes from `T`, which is the property the type
    /// parameter is here for.
    pub(crate) const fn read(name: &'static str, group: u8, number: u8) -> Self {
        Self {
            opcode: opcode::read::<T>(group, number),
            name,
            payload: PhantomData,
        }
    }
}

/// A request whose payload is a byte buffer, and which does not know how long that buffer is.
///
/// The absent length is deliberate. `EVIOCGKEY(len)` and its neighbours put the length into the
/// request number, and the kernel writes that many bytes: `bits_to_user` copies as many bytes as
/// the number encodes, bounded only by the length of the map it is copying. A number built for
/// ninety-six bytes and a buffer of four is therefore ninety-two bytes written past the end of the
/// buffer, from a safe function, with no `unsafe` token anywhere near the call.
///
/// So the length is never written down twice. It lives in the buffer, [`issue_bytes`] derives the
/// number from that buffer, and there is no way to spell a second one.
///
/// Every request in this family reads, so there is no direction to choose either. A direction
/// parameter here could express only a mistake.
#[derive(Clone, Copy)]
pub(crate) struct ByteRequest {
    /// What this is, for the error message.
    name: &'static str,
    /// The character the number is grouped under.
    group: u8,
    /// The number, which for `EVIOCGBIT` already carries the event type.
    number: u8,
}

impl ByteRequest {
    /// Returns the request number for a buffer of `len` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unusable`] when `len` does not fit the fourteen bits a request number has
    /// for it. `rustix` masks an oversized length into those bits rather than refusing, so sixteen
    /// kilobytes would quietly become a request for none.
    fn opcode(self, len: usize) -> Result<Opcode> {
        if len > MAX_PAYLOAD {
            return Err(Error::Unusable(format!(
                "{} was given {len} bytes, and a request number carries at most {MAX_PAYLOAD}",
                self.name
            )));
        }
        Ok(opcode::from_components(
            Direction::Read,
            self.group,
            self.number,
            len,
        ))
    }
}

/// Names a request the kernel writes its payload back through.
macro_rules! read_only {
    ($name:ident, $group:expr, $number:expr, $payload:ty) => {
        pub(crate) const $name: Request<$payload> =
            Request::read(stringify!($name), $group, $number);
    };
}

/// Names a request that only carries its payload to the kernel.
macro_rules! write_only {
    ($name:ident, $group:expr, $number:expr, $payload:ty) => {
        pub(crate) const $name: Request<$payload> = Request {
            opcode: opcode::write::<$payload>($group, $number),
            name: stringify!($name),
            payload: PhantomData,
        };
    };
}

/// Names a request whose argument is the integer the kernel reads.
///
/// The number is `_IOW(…, int)` all the same, so it is computed from `c_int` exactly as the
/// kernel's own macro does. Only the call differs.
macro_rules! by_value {
    ($name:ident, $group:expr, $number:expr) => {
        pub(crate) const $name: ValueRequest = ValueRequest {
            opcode: opcode::write::<c_int>($group, $number),
            name: stringify!($name),
        };
    };
}

/// Names a request that carries no payload.
///
/// The kernel reads and writes nothing through an `_IO` number, and `()` says the same.
macro_rules! no_payload {
    ($name:ident, $group:expr, $number:expr) => {
        pub(crate) const $name: Request<()> = Request {
            opcode: opcode::none($group, $number),
            name: stringify!($name),
            payload: PhantomData,
        };
    };
}

read_only!(GET_VERSION, GROUP, 0x01, c_int);
read_only!(GET_ID, GROUP, 0x02, sys::input_id);
by_value!(GRAB, GROUP, 0x90);

no_payload!(UINPUT_CREATE, UINPUT, 1);
no_payload!(UINPUT_DESTROY, UINPUT, 2);
write_only!(UINPUT_SETUP, UINPUT, 3, sys::uinput_setup);
write_only!(UINPUT_ABS_SETUP, UINPUT, 4, sys::uinput_abs_setup);
by_value!(UINPUT_SET_EVENT_BIT, UINPUT, 100);
by_value!(UINPUT_SET_KEY_BIT, UINPUT, 101);
by_value!(UINPUT_SET_RELATIVE_BIT, UINPUT, 102);
by_value!(UINPUT_SET_ABSOLUTE_BIT, UINPUT, 103);

/// Returns `EVIOCGNAME`, which answers the device's name into whatever buffer it is issued with.
pub(crate) const fn name() -> ByteRequest {
    ByteRequest {
        name: "EVIOCGNAME",
        group: GROUP,
        number: 0x06,
    }
}

/// Returns `EVIOCGKEY`, which answers which keys are held down right now, as a bitmap.
pub(crate) const fn key_state() -> ByteRequest {
    ByteRequest {
        name: "EVIOCGKEY",
        group: GROUP,
        number: 0x18,
    }
}

/// Returns `EVIOCGBIT(kind)`, which answers which codes of `kind` the device emits, as a bitmap.
///
/// A `kind` of zero asks which event types the device has at all: the kernel packs two questions
/// into one request.
///
/// # Errors
///
/// Returns [`Error::Unusable`] when `kind` is past `EV_MAX`, where the number would run into
/// `EVIOCGABS`'s range and the kernel would answer a different question.
pub(crate) fn bits(kind: u16) -> Result<ByteRequest> {
    let number = u8::try_from(kind)
        .ok()
        .filter(|kind| u32::from(*kind) <= sys::EV_MAX)
        .and_then(|kind| BIT_BASE.checked_add(kind))
        .ok_or_else(|| {
            Error::Unusable(format!(
                "EVIOCGBIT was asked for event type {kind}, and the last one is {}",
                sys::EV_MAX
            ))
        })?;
    Ok(ByteRequest {
        name: "EVIOCGBIT",
        group: GROUP,
        number,
    })
}

/// Returns `EVIOCGABS(axis)`, which answers the range and the current value of one absolute axis.
///
/// # Errors
///
/// Returns [`Error::Unusable`] when `axis` is past `ABS_MAX`.
pub(crate) fn absolute(axis: u16) -> Result<Request<sys::input_absinfo>> {
    let number = u8::try_from(axis)
        .ok()
        .filter(|axis| u32::from(*axis) <= sys::ABS_MAX)
        .and_then(|axis| ABS_BASE.checked_add(axis))
        .ok_or_else(|| {
            Error::Unusable(format!(
                "EVIOCGABS was asked for axis {axis}, and the last one is {}",
                sys::ABS_MAX
            ))
        })?;
    Ok(Request::read("EVIOCGABS", GROUP, number))
}

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
// construction. `IS_MUTATING` is true because the reading requests here have the kernel write back
// into the payload.
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

/// One ioctl whose payload is a byte buffer.
struct BytesCall<'a> {
    /// The request number, computed from `buffer`.
    opcode: Opcode,
    /// What the kernel writes into.
    buffer: &'a mut [u8],
}

// SAFETY: `as_ptr` hands back the start of `buffer`, which is a live `&mut [u8]` for the whole
// call, so it is valid, aligned and uniquely borrowed. The length the kernel reads out of the
// request number is `buffer.len()`, because `issue_bytes` is the only place a `BytesCall` is
// built and it computes the number from the same slice it stores here — there is no second
// length for the two to disagree about. The output is the number of bytes the kernel wrote, and
// `ioctl` returns that number for these requests.
unsafe impl Ioctl for BytesCall<'_> {
    type Output = usize;

    const IS_MUTATING: bool = true;

    fn opcode(&self) -> Opcode {
        self.opcode
    }

    fn as_ptr(&mut self) -> *mut c_void {
        self.buffer.as_mut_ptr().cast()
    }

    unsafe fn output_from_ptr(written: IoctlOutput, _: *mut c_void) -> rustix::io::Result<usize> {
        Ok(usize::try_from(written).unwrap_or(0))
    }
}

/// One ioctl whose argument is the value rather than a pointer to it.
struct ValueCall {
    /// The request number.
    opcode: Opcode,
    /// What the kernel reads the argument as.
    argument: c_int,
}

// SAFETY: nothing is dereferenced. `as_ptr` hands back the argument itself, which the kernel reads
// for the requests `ValueCall` serves: evdev's `EVIOCGRAB` branches on whether the argument is
// null, and uinput's `UI_SET_*BIT` family takes the code as the argument. Both are `_IOW(…, int)`
// numbers, so `IS_MUTATING` is true and nothing is written back.
unsafe impl Ioctl for ValueCall {
    type Output = ();

    const IS_MUTATING: bool = true;

    fn opcode(&self) -> Opcode {
        self.opcode
    }

    fn as_ptr(&mut self) -> *mut c_void {
        std::ptr::without_provenance_mut(self.argument as usize)
    }

    unsafe fn output_from_ptr(_: IoctlOutput, _: *mut c_void) -> rustix::io::Result<()> {
        Ok(())
    }
}

/// Issues `request` against `fd`, with `payload` as its argument.
///
/// A call is restarted on `EINTR` and on `EAGAIN`. The kernel returns the first when a signal
/// arrives mid-call and the second when the device is busy, and neither is a failure of the
/// request: a caller that did not loop would see a working machine fail at random under load or
/// under a profiler.
pub(crate) fn issue<T>(fd: BorrowedFd<'_>, request: Request<T>, payload: &mut T) -> Result<()> {
    retry(request.name, || {
        // SAFETY: the claims are stated on the `Ioctl` implementation above. `fd` is a live
        // borrowed descriptor for the duration of the call.
        unsafe {
            rustix::ioctl::ioctl(
                fd,
                Call {
                    opcode: request.opcode(),
                    payload: &mut *payload,
                },
            )
        }
    })
}

/// Issues `request` against `fd`, filling `buffer`, and reports how many bytes the kernel wrote.
///
/// The request number is built here, from `buffer`. That is the only length there is, so the
/// number and the buffer cannot disagree about how much the kernel may write.
///
/// # Errors
///
/// Returns [`Error::Unusable`] when `buffer` is longer than a request number can carry, and
/// [`Error::Ioctl`] when the kernel refuses.
pub(crate) fn issue_bytes(
    fd: BorrowedFd<'_>,
    request: ByteRequest,
    buffer: &mut [u8],
) -> Result<usize> {
    let opcode = request.opcode(buffer.len())?;
    retry(request.name, || {
        // SAFETY: the claims are stated on the `Ioctl` implementation above.
        unsafe {
            rustix::ioctl::ioctl(
                fd,
                BytesCall {
                    opcode,
                    buffer: &mut *buffer,
                },
            )
        }
    })
}

/// Issues `request` against `fd`, with `value` as the argument itself.
///
/// Nothing is pointed at. [`ValueRequest`] is the only request type this accepts and the only one
/// [`issue`] refuses, so the two shapes cannot be confused at a call site.
///
/// # Errors
///
/// Returns [`Error::Ioctl`] when the kernel refuses.
pub(crate) fn issue_value(fd: BorrowedFd<'_>, request: ValueRequest, value: c_int) -> Result<()> {
    retry(request.name, || {
        // SAFETY: the claims are stated on the `Ioctl` implementation above.
        unsafe {
            rustix::ioctl::ioctl(
                fd,
                ValueCall {
                    opcode: request.opcode(),
                    argument: value,
                },
            )
        }
    })
}

/// Runs `call` until it answers with something other than `EINTR` or `EAGAIN`.
fn retry<T>(name: &'static str, mut call: impl FnMut() -> rustix::io::Result<T>) -> Result<T> {
    loop {
        return match call() {
            Ok(output) => Ok(output),
            Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => continue,
            Err(errno) => Err(Error::Ioctl {
                request: name,
                source: errno.into(),
            }),
        };
    }
}

#[cfg(test)]
mod tests {
    //! The request numbers, against the values the headers produce.
    //!
    //! These are what `EVIOCGID` and its neighbours expand to when the C preprocessor is run over
    //! the kernel's own headers. A struct generated at the wrong size changes the number, and this
    //! is where that shows up as a failure rather than as `EINVAL` from a device.

    use super::*;

    #[test]
    fn the_request_numbers_are_the_ones_the_headers_expand_to() {
        assert_eq!(GET_VERSION.opcode(), 0x8004_4501);
        assert_eq!(GET_ID.opcode(), 0x8008_4502);
        assert_eq!(GRAB.opcode(), 0x4004_4590);
    }

    #[test]
    fn the_length_of_the_buffer_lands_in_the_request_number() {
        // `EVIOCGNAME(256)`, `EVIOCGNAME(1)` and `EVIOCGKEY(96)`. The length is the middle
        // fourteen bits, so a buffer of a different length is a different request — which is why
        // the number is computed from the buffer and nowhere else.
        assert_eq!(name().opcode(256).expect("256 fits"), 0x8100_4506);
        assert_eq!(name().opcode(1).expect("1 fits"), 0x8001_4506);
        assert_eq!(key_state().opcode(96).expect("96 fits"), 0x8060_4518);
    }

    #[test]
    fn an_event_type_and_an_axis_land_in_the_request_number() {
        // `EVIOCGBIT(0, 4)` asks which event types there are; the rest ask for the codes of one.
        let opcode = |kind, len| {
            bits(kind)
                .expect("the type is one the kernel has")
                .opcode(len)
                .expect("the length fits")
        };
        assert_eq!(opcode(0, 4), 0x8004_4520);
        assert_eq!(opcode(1, 96), 0x8060_4521);
        assert_eq!(opcode(2, 2), 0x8002_4522);
        assert_eq!(opcode(3, 8), 0x8008_4523);
        assert_eq!(absolute(0).expect("ABS_X is an axis").opcode(), 0x8018_4540);
        assert_eq!(absolute(1).expect("ABS_Y is an axis").opcode(), 0x8018_4541);
    }

    #[test]
    fn the_uinput_request_numbers_are_the_ones_the_headers_expand_to() {
        assert_eq!(UINPUT_CREATE.opcode(), 0x0000_5501);
        assert_eq!(UINPUT_DESTROY.opcode(), 0x0000_5502);
        assert_eq!(UINPUT_SETUP.opcode(), 0x405c_5503);
        assert_eq!(UINPUT_ABS_SETUP.opcode(), 0x401c_5504);
        assert_eq!(UINPUT_SET_EVENT_BIT.opcode(), 0x4004_5564);
        assert_eq!(UINPUT_SET_KEY_BIT.opcode(), 0x4004_5565);
        assert_eq!(UINPUT_SET_RELATIVE_BIT.opcode(), 0x4004_5566);
        assert_eq!(UINPUT_SET_ABSOLUTE_BIT.opcode(), 0x4004_5567);
    }

    #[test]
    fn a_payload_longer_than_a_request_number_can_carry_is_refused() {
        // `rustix` masks the size into fourteen bits rather than refusing, so 16384 bytes would
        // silently become a request for zero. The kernel would then answer a question nobody
        // asked, with the buffer left as it was.
        assert_eq!(
            (name().opcode(MAX_PAYLOAD).expect("the limit fits") >> 16) & 0x3fff,
            0x3fff,
            "the whole of the size field is used"
        );
        assert!(
            name().opcode(MAX_PAYLOAD + 1).is_err(),
            "one byte past the limit is refused rather than wrapped"
        );
    }

    #[test]
    fn a_type_or_an_axis_the_kernel_does_not_have_is_refused() {
        // Past `EV_MAX` the number runs into `EVIOCGABS`'s range, so the kernel would answer a
        // different question. The same holds for an axis past `ABS_MAX`.
        assert!(bits(32).is_err(), "`EV_MAX` is 31");
        assert!(absolute(64).is_err(), "`ABS_MAX` is 63");
    }

    #[test]
    fn the_three_call_shapes_reach_a_live_kernel_object() {
        // `/dev/uinput` is the one input device a program can make for itself, so it is how the
        // table above is exercised against a kernel rather than against arithmetic. All three
        // shapes are here: `UI_SET_EVBIT` passes the code *as* the argument, `UI_DEV_SETUP`
        // passes a ninety-two byte structure by pointer, and `UI_DEV_CREATE` passes nothing.
        //
        // What it deliberately does not do is read the device back. The node the kernel makes
        // belongs to `root:input`, so the process that created it usually cannot open it.
        let node = rustix::fs::open(
            "/dev/uinput",
            rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        );
        let Ok(node) = node else {
            eprintln!(
                "the_three_call_shapes_reach_a_live_kernel_object: /dev/uinput cannot be opened \
                 on this machine, so nothing was asserted; load the module with `sudo modprobe \
                 uinput` and grant write access to run it"
            );
            return;
        };
        let fd = rustix::fd::AsFd::as_fd(&node);

        issue_value(fd, UINPUT_SET_EVENT_BIT, sys::EV_KEY as c_int)
            .expect("a synthetic device may say it has keys");
        issue_value(fd, UINPUT_SET_KEY_BIT, sys::KEY_A as c_int)
            .expect("a synthetic device may say which key");

        let mut setup = sys::uinput_setup {
            id: sys::input_id {
                bustype: sys::BUS_VIRTUAL as u16,
                vendor: 0x1209,
                product: 0x0001,
                version: 1,
            },
            ff_effects_max: 0,
            ..Default::default()
        };
        for (slot, byte) in setup.name.iter_mut().zip(b"zgui-evdev request table") {
            *slot = *byte as core::ffi::c_char;
        }
        issue(fd, UINPUT_SETUP, &mut setup).expect("a device description is accepted");

        issue(fd, UINPUT_CREATE, &mut ()).expect("the device is created");
        issue(fd, UINPUT_DESTROY, &mut ()).expect("the device is destroyed");
    }

    /// Compiles only if `request` may be issued with a payload of type `T`.
    fn assert_payload<T>(_: Request<T>) {}

    #[test]
    fn a_request_carries_the_payload_type_its_number_was_computed_for() {
        // What this refuses is the point: `assert_payload::<c_int>(UINPUT_SETUP)` does not compile,
        // so a call site cannot pair a number with a payload smaller than the bytes the kernel
        // writes back through it. `assert_payload` refuses a `Request<[u8]>` outright, which keeps
        // the run-time length out of the sized table.
        assert_payload::<sys::input_id>(GET_ID);
        assert_payload::<sys::uinput_setup>(UINPUT_SETUP);
        assert_payload::<()>(UINPUT_CREATE);
    }
}
