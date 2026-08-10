//! What a seat does, on a backend that needs no session.
//!
//! `LIBSEAT_BACKEND=noop` is a seat that opens, enables at once, and asks nothing of the machine:
//! no daemon, no root, and no terminal. So the open, the wait for the first enable, the name, the
//! descriptor, the queue and the close are all reachable from an ordinary shell, and that is what
//! this binary covers. What noop cannot reach is a disable and the enable that follows it, which
//! need a session and a second terminal.
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

use std::env;
use std::os::fd::AsRawFd;
use std::time::Instant;

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

    if !INSTALLED_AS.into_iter().any(is_installed) {
        eprintln!(
            "noop: this machine has no libseat, so a seat was opened against nothing and none of \
             the six checks ran. Install seatd, or run the suite from `nix develop`, which puts \
             `libseat.so.1` on the library path."
        );
        return;
    }

    let checks: [(&str, fn()); 6] = [
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
        ("dropping_the_seat_closes_it", dropping_the_seat_closes_it),
        (
            "a_backend_nothing_has_is_refused",
            a_backend_nothing_has_is_refused,
        ),
    ];

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

/// Dropping the seat gives its descriptors back.
///
/// The noop backend's connection is one end of a socket pair it makes when the seat opens, and
/// closing the seat closes it. The system hands out the lowest free number, so a second seat opened
/// after the first is dropped is given the number the first one had. A seat that was never closed
/// holds that number, and the second seat gets a higher one.
fn dropping_the_seat_closes_it() {
    let first = open();
    let descriptor = first.descriptor().as_raw_fd();
    drop(first);

    let second = open();

    assert_eq!(
        second.descriptor().as_raw_fd(),
        descriptor,
        "the first seat was closed, so its descriptor was free for the second one to be given"
    );
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
