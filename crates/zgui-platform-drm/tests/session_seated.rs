//! What a session answers where a seat opens, and where the card comes from then.
//!
//! `LIBSEAT_BACKEND=noop` is a seat that opens, enables at once, and asks nothing of the machine:
//! no daemon, no root, and no terminal. It opens a device with a plain `open(2)`, so the card comes
//! back with no DRM master on it. Nothing below sets a mode, so none is needed.
//!
//! # Why this binary has its own `main`
//!
//! `harness = false`, because the backend is chosen through an environment variable. Writing one
//! while another thread reads the environment is a data race, and a test harness runs its tests on
//! several threads. Here the variable is set by the first statement of the process, before any
//! thread exists and before libseat is opened.
//!
//! # What noop leaves uncovered
//!
//! **Master arriving with the card.** logind and seatd set it before they answer the client and
//! noop sets nothing, so a card here is a card without master. That the session asks for none is
//! what is covered; that it already has one is read on hardware.
//!
//! **`libseat_close_device` being called at all.** noop's is `return 0;`, so what is asserted below
//! is the descriptor going back. The daemon's record of the device is invisible from here, and a
//! session that told libseat nothing passes every check in this file. Deleting the give-back loop
//! from the session's own `Drop` leaves this binary passing for the same reason: the devices it
//! takes out of the session would be dropped by that body anyway, and dropping one closes its
//! descriptor. The loop releases the daemon's record while the seat stays open, and `loginctl
//! session-status` is where that is read.

// The environment is written once below, before this process has made a thread. The block states
// what makes it sound where it is made.
#![allow(unsafe_code)]

mod support;

use std::env;
use std::os::fd::{AsFd, AsRawFd};
use std::path::PathBuf;
use std::sync::Arc;

use zgui_platform_drm::Session;

/// The variable libseat reads the backend name out of.
const BACKEND: &str = "LIBSEAT_BACKEND";

/// A path under `/dev/dri` that names no device, which a seat refuses to open at all.
const ABSENT: &str = "/dev/dri/card-nothing-is-here";

/// A path every user opens and no driver answers a DRM request on.
const NOT_A_CARD: &str = "/dev/null";

/// Sets the backend, and then runs every check the machine can answer.
///
/// A check that fails panics, which ends the run and reports the process as failed.
fn main() {
    // SAFETY: this is the first statement of the process. No thread has been made yet, so nothing
    // else can read the environment while it is written, and libseat is opened by the checks below,
    // which is after this returns.
    unsafe { env::set_var(BACKEND, "noop") };

    if !support::libseat_is_installed() {
        eprintln!(
            "session_seated: this machine has no libseat, so no seat was opened and none of the \
             checks ran. Install seatd, or run the suite from `nix develop`, which puts \
             `libseat.so.1` on the library path."
        );
        return;
    }

    println!("session_seated: a_seat_is_the_shape_this_run_gets");
    a_seat_is_the_shape_this_run_gets();

    let cards = support::openable_cards();
    if cards.is_empty() {
        eprintln!(
            "session_seated: no `card*` under /dev/dri opens for this process, so where the card \
             comes from was not asserted. The noop backend opens a device with a plain `open(2)`, \
             so this needs a card this user may read and write: add one with `sudo modprobe vkms`, \
             or run from a seat whose session holds the console's own card."
        );
        return;
    }

    println!("session_seated: the_card_comes_from_the_seat_and_goes_back_to_it");
    the_card_comes_from_the_seat_and_goes_back_to_it(&cards);

    println!("session_seated: the_card_and_the_seats_device_are_one_open_file_description");
    the_card_and_the_seats_device_are_one_open_file_description(&cards);

    println!("session_seated: a_path_that_is_no_card_goes_back_and_the_walk_carries_on");
    a_path_that_is_no_card_goes_back_and_the_walk_carries_on(&cards);

    println!("session_seated: a_descriptor_that_cannot_be_copied_goes_back_to_the_seat");
    a_descriptor_that_cannot_be_copied_goes_back_to_the_seat(&cards);

    println!("session_seated: one_session_answers_one_card");
    one_session_answers_one_card(&cards);

    println!("session_seated: 6 checks passed");
}

/// A run that opened a seat is a seated run.
///
/// It is the one check this binary shares with `session_direct`, and the two answers together say
/// that [`Session::open`] reads the machine rather than answering one shape always.
fn a_seat_is_the_shape_this_run_gets() {
    let session = Session::open();

    assert!(
        session.is_seated(),
        "libseat opened a seat on the noop backend, so the devices come from it"
    );
}

/// The card is opened through the seat, held by the session, and released with it.
///
/// Descriptor counting says all three. One card opened this way is **two** descriptors: the one the
/// seat opened, which the session keeps because giving the device back goes through it, and the
/// duplicate the display device is built over. The session keeps a name on that one too, because
/// the master and the console are handed back through it, so a caller that drops its own name on
/// the card closes nothing.
///
/// The last count says the descriptors went back. Whether the daemon was told is a different fact,
/// and this backend cannot show it — see what noop leaves uncovered, above.
fn the_card_comes_from_the_seat_and_goes_back_to_it(cards: &[PathBuf]) {
    let before = support::open_descriptors();
    let mut session = Session::open();
    let seated = support::open_descriptors();

    let card = session.card().unwrap_or_else(|error| {
        panic!(
            "the noop backend opens any path it is given, and this machine has {} to open: {error}",
            cards.len()
        )
    });

    assert!(
        cards.iter().any(|path| path == card.path()),
        "the device names the card the seat opened, and this machine's are {cards:?}, not {}",
        card.path().display()
    );
    assert_eq!(
        support::open_descriptors(),
        seated + 2,
        "one card is two descriptors: the one the seat opened and the duplicate the device is \
         built over"
    );

    drop(card);

    assert_eq!(
        support::open_descriptors(),
        seated + 2,
        "the session holds the seat's own device and a name on the card, so what a caller lets go \
         of closes neither"
    );

    drop(session);

    assert_eq!(
        support::open_descriptors(),
        before,
        "the session went, so every descriptor the card cost went with it: the seat's own device \
         and the seat"
    );
}

/// The device is built over the descriptor the seat handed across, and not over one of its own.
///
/// This is what the handover is for. DRM master, the client capabilities and the status flags live
/// on the open file description, so a device built over a **copy** of the seat's descriptor is the
/// master the daemon granted seen through a second name — and a device that opened the card again
/// is a second description with none of it. On logind that second one has no master; where the card
/// is reachable through the daemon alone it does not open at all. Both machines are out of reach
/// here, and `kcmp(2)` answers the same question on this one.
///
/// Counting the two descriptors is part of the assertion. The card path names exactly the seat's
/// device and the duplicate, so a third name for it would be a descriptor nothing here accounts
/// for.
fn the_card_and_the_seats_device_are_one_open_file_description(cards: &[PathBuf]) {
    let mut session = Session::open();

    let card = session
        .card()
        .unwrap_or_else(|error| panic!("this machine has {} cards to open: {error}", cards.len()));

    let held = support::descriptors_naming(card.path());
    assert_eq!(
        held.len(),
        2,
        "the card is named by the seat's own descriptor and by the duplicate, and {} names it",
        held.len()
    );

    let card_descriptor = card.as_fd().as_raw_fd();
    let seats = held
        .into_iter()
        .find(|held| *held != card_descriptor)
        .expect("one of the two descriptors on the card is the device's own");

    match support::one_open_file_description(card_descriptor, seats) {
        Some(true) => {}
        Some(false) => panic!(
            "the device was built over a descriptor of its own, so the master and the capabilities \
             a session daemon put on the descriptor it handed over reach nothing this run holds"
        ),
        None => eprintln!(
            "session_seated: this machine answered no `kcmp(2)`, so the two descriptors were \
             counted and not compared. It needs a kernel built with `CONFIG_CHECKPOINT_RESTORE` on \
             an architecture whose call number this suite writes out."
        ),
    }
}

/// A path the walk meets that is no card goes back to the seat, and the next path is tried.
///
/// A seat hands out input devices over the call it hands out cards with, so both refusals below are
/// answers a real machine gives. The first path names nothing, which the seat itself refuses; the
/// second opens and answers no DRM request, which `zgui-drm` refuses after the seat has already
/// opened it. The card behind them proves the walk carried on, and the count proves the device that
/// was refused went back rather than being kept.
fn a_path_that_is_no_card_goes_back_and_the_walk_carries_on(cards: &[PathBuf]) {
    let before = support::open_descriptors();
    let mut session = Session::open();
    let seated = support::open_descriptors();

    let mut walk = vec![PathBuf::from(ABSENT), PathBuf::from(NOT_A_CARD)];
    walk.extend(cards.iter().cloned());

    let card = session.card_from(&walk).unwrap_or_else(|error| {
        panic!(
            "the two refusals in front of {} cards ended the walk: {error}",
            cards.len()
        )
    });

    assert!(
        cards.iter().any(|path| path == card.path()),
        "the walk answered {}, which is one of the two paths that are no card",
        card.path().display()
    );
    assert_eq!(
        support::open_descriptors(),
        seated + 2,
        "the refused device went back to the seat, so what this session holds is one card: the \
         seat's own descriptor and the duplicate"
    );

    drop(card);
    drop(session);

    assert_eq!(
        support::open_descriptors(),
        before,
        "the session went, and nothing the refused paths cost outlived it"
    );
}

/// A descriptor the seat opened that cannot be copied goes back before the walk moves on.
///
/// The copy is where the seat's descriptor is turned into the one the display device owns, and it
/// is a descriptor of its own — so a process at its limit reaches this refusal with the seat's
/// device already open. The limit is set one above the lowest number this process has free, which
/// leaves room for the seat's `open(2)` and none for the copy.
fn a_descriptor_that_cannot_be_copied_goes_back_to_the_seat(cards: &[PathBuf]) {
    let before = support::open_descriptors();
    let mut session = Session::open();
    let seated = support::open_descriptors();

    let room_for_one = support::lowest_free_descriptor() + 1;
    let refusal = support::while_the_limit_is(room_for_one, || session.card_from(cards))
        .expect_err("the copy of the seat's descriptor is one descriptor past the limit");

    assert!(
        refusal.to_string().contains("cannot be copied"),
        "the refusal names the copy that failed, and it reads: {refusal}"
    );
    assert_eq!(
        support::open_descriptors(),
        seated,
        "every device the seat opened during the walk went back to it"
    );

    drop(session);

    assert_eq!(
        support::open_descriptors(),
        before,
        "the session went, and the seat with it"
    );
}

/// One session drives one card, and asks for it once.
///
/// A second walk would ask the daemon for a device it has already handed over, which logind
/// refuses, and would leave a second device to give back. So the card is taken on the first call
/// and every later call answers it.
fn one_session_answers_one_card(cards: &[PathBuf]) {
    let mut session = Session::open();

    let first = session
        .card()
        .unwrap_or_else(|error| panic!("this machine has {} cards to open: {error}", cards.len()));
    let taken = support::open_descriptors();
    let again = session
        .card()
        .expect("a session that has taken its card answers it again");

    assert!(
        Arc::ptr_eq(&first, &again),
        "both calls answer one card: {} and {}",
        first.path().display(),
        again.path().display()
    );
    assert_eq!(
        support::open_descriptors(),
        taken,
        "the second call opened nothing, so it asked the seat for nothing"
    );
}
