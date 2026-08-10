//! What a seat does, on a backend that needs no session.
//!
//! `LIBSEAT_BACKEND=noop` is a seat that opens, enables at once, and asks nothing of the machine:
//! no daemon, no root, and no terminal. So the open, the wait for the first enable, the name, the
//! descriptor, the queue and the close are all reachable from an ordinary shell, and that is what
//! this binary covers.
//!
//! # What noop leaves uncovered
//!
//! noop produces one change in the life of a seat, and the open consumes it. So no change of any
//! kind arrives through `Seat::dispatch` here: a disable, the enable that follows it, the order of
//! the two and a queue holding more than one entry are all outside what this backend can reach, and
//! they need a session and a second terminal. What is covered of `dispatch` is that it reaches
//! libseat and reports what libseat refused. A run on a machine with terminals covers what turns an
//! arriving change into a `Change`.
//!
//! # Why this binary has its own `main`
//!
//! `harness = false`, because the backend is chosen through an environment variable. Writing one
//! while another thread reads the environment is a data race, and a test harness runs its tests on
//! several threads. Here the variable is set by the first statement of the process, before any
//! thread exists and before libseat is opened.
//!
//! # What may be skipped
//!
//! Whether libseat is on the machine is asked of the loader directly, through [`is_installed`].
//! Reading that decision out of the answer the code under test gave would send every regression
//! into the silent arm, where the suite stays green over an assertion nobody makes. Once libseat is
//! here, every check below is an assertion: libseat compiles its noop backend unconditionally, so a
//! libseat that opens is a libseat that has one.

// The environment is written once below, and the loader is asked one question directly. Both are
// unsafe calls, and both state what makes them sound where they are made.
#![allow(unsafe_code)]

use std::ffi::c_int;
use std::os::fd::AsRawFd;
use std::time::Instant;
use std::{env, fs};

use zgui_seat::{ENABLE_WITHIN, Error, Seat};

/// The variable libseat reads the backend name out of.
const BACKEND: &str = "LIBSEAT_BACKEND";

/// The two names libseat is installed under, written out.
///
/// Deliberately apart from `zgui_seat::library::SONAMES`. This is what the machine is asked about,
/// so a wrong `SONAMES` cannot decide that the machine has no libseat and send this binary into the
/// silent arm.
const INSTALLED_AS: [&str; 2] = ["libseat.so.1", "libseat.so"];

/// Sets the backend, and then runs every check in turn.
///
/// A check that fails panics, which ends the run and reports the process as failed. The name is
/// printed before the check runs, so the last name printed is the one that failed.
fn main() {
    // SAFETY: this is the first statement of the process. No thread has been made yet, so nothing
    // else can read the environment while it is written, and libseat is opened by the checks below,
    // which is after this returns.
    unsafe { env::set_var(BACKEND, "noop") };

    let checks: [(&str, fn()); 7] = [
        (
            "a_seat_opens_and_enables_inside_the_bound",
            a_seat_opens_and_enables_inside_the_bound,
        ),
        ("the_seat_is_named", the_seat_is_named),
        (
            "the_descriptor_is_an_open_one",
            the_descriptor_is_an_open_one,
        ),
        (
            "a_quiet_dispatch_answers_no_change",
            a_quiet_dispatch_answers_no_change,
        ),
        (
            "a_refused_dispatch_is_reported",
            a_refused_dispatch_is_reported,
        ),
        ("dropping_the_seat_closes_it", dropping_the_seat_closes_it),
        (
            "a_backend_nothing_has_is_refused",
            a_backend_nothing_has_is_refused,
        ),
    ];

    if !INSTALLED_AS.into_iter().any(is_installed) {
        eprintln!(
            "noop: this machine has no libseat, so no seat was opened and none of the {} checks \
             ran. Install seatd, or run the suite from `nix develop`, which puts `libseat.so.1` on \
             the library path.",
            checks.len()
        );
        return;
    }

    for (name, check) in checks {
        println!("noop: {name}");
        check();
    }

    println!("noop: {} checks passed", checks.len());
}

/// Returns `true` if this soname opens on this machine, asked of the loader directly.
///
/// This goes around [`Seat`] and around `zgui_seat::Library` on purpose. It is the precondition of
/// every check below, and a binary that asked the subject whether its own precondition holds would
/// skip itself exactly when the subject broke.
fn is_installed(soname: &str) -> bool {
    // SAFETY: opening a shared object runs the initialisers of its whole dependency closure. This
    // is libseat, which does no work of its own at load time, and below it sit the C library and
    // libsystemd. The handle is dropped straight away.
    unsafe { libloading::Library::new(soname) }.is_ok()
}

/// Opens a seat, and panics with what refused it.
fn open() -> Seat {
    Seat::open().unwrap_or_else(|error| {
        panic!("libseat is on this machine and its noop backend needs nothing, so a seat opens: {error}")
    })
}

/// The open answers a seat, and the seat is enabled by the time it does.
fn a_seat_opens_and_enables_inside_the_bound() {
    let started = Instant::now();
    let seat = open();
    let took = started.elapsed();

    assert!(
        took < ENABLE_WITHIN,
        "the seat enabled inside the bound, and this one took {took:?} of {ENABLE_WITHIN:?}"
    );
    drop(seat);
}

/// The seat carries the name its backend gave it.
fn the_seat_is_named() {
    let seat = open();

    assert!(!seat.name().is_empty(), "the seat has a name");
    assert_eq!(
        seat.name(),
        "seat0",
        "and the noop backend answers a fixed one"
    );
}

/// The descriptor is a live one, which a loop can wait on.
fn the_descriptor_is_an_open_one() {
    let seat = open();
    let descriptor = seat.descriptor();

    assert!(
        descriptor.as_raw_fd() >= 0,
        "a descriptor is a number the system gave: {}",
        descriptor.as_raw_fd()
    );

    // Copying a descriptor is refused for one that is closed, so this says the number names
    // something open rather than merely being a plausible number.
    let copy = descriptor
        .try_clone_to_owned()
        .unwrap_or_else(|error| panic!("the seat's descriptor is open, so it copies: {error}"));

    assert_ne!(
        copy.as_raw_fd(),
        descriptor.as_raw_fd(),
        "a copy is a second descriptor"
    );
}

/// A seat that nothing has happened to reports nothing.
fn a_quiet_dispatch_answers_no_change() {
    let mut seat = open();

    for turn in 0..3 {
        let changes = seat
            .dispatch()
            .unwrap_or_else(|error| panic!("a quiet seat dispatches: {error}"));

        assert!(
            changes.is_empty(),
            "nothing has happened to this seat, and turn {turn} answered {changes:?}. The enable \
             the open waited for is the open's, and is consumed there."
        );
    }
}

/// A dispatch libseat refuses is answered as a refusal.
///
/// noop makes one thing visible about [`Seat::dispatch`]: that the call reaches libseat. A
/// `dispatch` that answered an empty list and called nothing satisfies every other check in this
/// file, because noop delivers no change after the open.
///
/// noop's dispatch polls the socket pair it made at the open, on every call and whatever the timeout
/// is. Linux refuses a `poll` for more descriptors than [`DESCRIPTOR_LIMIT`] permits, so a limit of
/// zero makes that poll fail with [`EINVAL`] and noop answers `-1`. The limit is put back before
/// anything is asserted, because a process that may open no descriptor cannot report a failure
/// either.
fn a_refused_dispatch_is_reported() {
    let mut seat = open();
    let limit = descriptor_limit();

    set_descriptor_limit(&Rlimit {
        current: 0,
        maximum: limit.maximum,
    });
    let answer = seat.dispatch();
    set_descriptor_limit(&limit);

    match answer.expect_err("`poll` was refused, so the dispatch that makes it failed") {
        Error::Dispatch { errno } => assert_eq!(
            errno, EINVAL,
            "the number is the one `poll` answers for a descriptor limit it cannot meet"
        ),
        other => panic!("a dispatch that failed is reported as one: {other}"),
    }
}

/// Dropping the seat gives its descriptors back.
///
/// The noop backend's connection is one end of a socket pair it makes when the seat opens, and
/// `libseat_close_seat` closes both ends. So the number of descriptors this process holds rises
/// across an open and is back where it started after the drop.
///
/// The count says so. The numbers the system hands out move whenever anything else opens a
/// descriptor of its own, so the assertion is over how many this process holds.
fn dropping_the_seat_closes_it() {
    let before = open_descriptors();

    let seat = open();
    let held = open_descriptors();
    assert!(
        held > before,
        "an open seat holds descriptors this process did not have: {before} before it, and {held} \
         while it is open"
    );

    drop(seat);

    assert_eq!(
        open_descriptors(),
        before,
        "the seat was closed, so every descriptor it took went back"
    );
}

/// Returns how many descriptors this process holds.
///
/// `/proc/self/fd` carries one entry per descriptor, and reading it holds one of its own, so the
/// answer is one higher than the number held. Every call here counts the same way and every
/// comparison is between two of them.
fn open_descriptors() -> usize {
    fs::read_dir("/proc/self/fd")
        .unwrap_or_else(|error| {
            panic!("libseat runs on Linux, which has `/proc/self/fd` to count: {error}")
        })
        .count()
}

/// A backend that cannot be found is a refusal rather than a seat.
///
/// This is the path a session that already has a controlling client takes, and it is where the
/// state the callbacks reach through is given back although no callback ever ran.
fn a_backend_nothing_has_is_refused() {
    println!("noop: a libseat log line about a missing backend belongs to this check");

    // SAFETY: this binary has made no thread, so nothing reads the environment while it is written.
    // See the comment in `main`.
    unsafe { env::set_var(BACKEND, "there-is-no-such-backend") };
    let error = Seat::open().err();
    // SAFETY: as above.
    unsafe { env::set_var(BACKEND, "noop") };

    match error.expect("libseat has no backend under that name, so it opens no seat") {
        Error::Seat { call, errno } => {
            assert_eq!(call, "libseat_open_seat", "the call that refused is named");
            assert_ne!(errno, 0, "and the system said why");
        }
        other => panic!("a refused seat is reported as one: {other}"),
    }
}

/// Linux's `RLIMIT_NOFILE`: how many descriptors this process may hold.
///
/// Written out, because the standard library carries no resource limits and this binary names no
/// crate that does. `7` is the kernel's generic numbering, which every architecture this suite runs
/// on uses. A few older ones number their limits their own way.
const DESCRIPTOR_LIMIT: c_int = 7;

/// `EINVAL`, the number `poll` answers for more descriptors than [`DESCRIPTOR_LIMIT`] permits.
///
/// The first thirty-four error numbers are the kernel's generic ones, which every architecture uses.
const EINVAL: i32 = 22;

/// The C library's `struct rlimit`.
///
/// `rlim_t` is eight bytes wide on the 64-bit targets this suite runs on.
#[repr(C)]
#[derive(Clone, Copy)]
struct Rlimit {
    /// The limit in force. A process lowers this for itself and raises it again up to `maximum`.
    current: u64,
    /// The ceiling on `current`. Only a privileged process raises this one.
    maximum: u64,
}

// The C library's own, which is where a resource limit is read and written. Both are declared here
// for the same reason libseat's interface is declared by hand: what crosses is stated once, beside
// the code that calls it.
unsafe extern "C" {
    /// `getrlimit(2)`. Writes the limit in force through the pointer, and answers `0` or `-1`.
    fn getrlimit(resource: c_int, limit: *mut Rlimit) -> c_int;
    /// `setrlimit(2)`. Puts the limit behind the pointer in force, and answers `0` or `-1`.
    fn setrlimit(resource: c_int, limit: *const Rlimit) -> c_int;
}

/// Returns the descriptor limit this process holds.
fn descriptor_limit() -> Rlimit {
    let mut limit = Rlimit {
        current: 0,
        maximum: 0,
    };

    // SAFETY: `getrlimit` writes one `struct rlimit` through the pointer, and this is one, owned by
    // this frame and live for the length of the call. The resource is a number the system defines.
    let answer = unsafe { getrlimit(DESCRIPTOR_LIMIT, &raw mut limit) };

    assert_eq!(answer, 0, "a process reads its own descriptor limit");
    limit
}

/// Puts a descriptor limit in force.
fn set_descriptor_limit(limit: &Rlimit) {
    // SAFETY: `setrlimit` reads one `struct rlimit` through the pointer, and this is one, live for
    // the length of the call. Lowering the limit in force, and raising it again up to the ceiling it
    // came with, is what a process does for itself.
    let answer = unsafe { setrlimit(DESCRIPTOR_LIMIT, limit) };

    assert_eq!(
        answer, 0,
        "a process lowers its own descriptor limit and puts it back"
    );
}
