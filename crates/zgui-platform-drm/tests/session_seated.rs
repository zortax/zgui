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
//! **Master arriving with the card.** logind grants it before it answers the client and noop grants
//! nothing, so a card here is a card without master. That the session asks for none is what is
//! covered; that it already has one is read on hardware.
//!
//! **`libseat_close_device` being called at all.** noop's is `return 0;`, so what is asserted below
//! is the descriptor going back. The daemon's record of the device is invisible from here, and a
//! session that told libseat nothing passes every check in this file.

// The environment is written once below, before this process has made a thread. The block states
// what makes it sound where it is made.
#![allow(unsafe_code)]

mod support;

use std::env;
use std::path::PathBuf;

use zgui_platform_drm::Session;

/// The variable libseat reads the backend name out of.
const BACKEND: &str = "LIBSEAT_BACKEND";

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
            "session_seated: this machine has no libseat, so no seat was opened and neither check \
             ran. Install seatd, or run the suite from `nix develop`, which puts `libseat.so.1` on \
             the library path."
        );
        return;
    }

    println!("session_seated: a_seat_leaves_the_console_and_the_master_alone");
    a_seat_leaves_the_console_and_the_master_alone();

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

    println!("session_seated: 2 checks passed");
}

/// A seated session takes neither the console nor DRM master.
///
/// Both belong to the session daemon on this path: it puts the terminal into graphics mode when it
/// grants control, and it sets master on the card before it hands the descriptor over. So the two
/// answers together are what says which shape the session got.
fn a_seat_leaves_the_console_and_the_master_alone() {
    let session = Session::open();

    assert!(
        !session.takes_the_console(),
        "a seat opened, so the daemon has the terminal and this session leaves it alone"
    );
    assert!(
        !session.takes_the_master(),
        "a seat opened, so master sits on the description the daemon granted it on and this \
         session gives up nothing"
    );
}

/// The card is opened through the seat, held by the session, and released with it.
///
/// Descriptor counting says all three. One card opened this way is **two** descriptors: the one the
/// seat opened, which the session holds so that it can give the device back, and the duplicate the
/// display device is built over. Both name one open file description, so the duplicate is the right
/// thing to hand over.
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
        seated + 1,
        "the session holds the seat's own device, so it outlives the display device built over a \
         copy of its descriptor"
    );

    drop(session);

    assert_eq!(
        support::open_descriptors(),
        before,
        "the session went, so every descriptor the card cost went with it: the seat's own device \
         and the seat"
    );
}
