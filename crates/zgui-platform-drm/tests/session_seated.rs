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
//!
//! **A device id that is answered twice.** seatd raises a reference count for a path the client
//! already holds and answers the id it answered before, which is why a device is closed before the
//! same path is opened again. noop and logind each answer a fresh id, so the ordering is asserted
//! here by the cost of a round rather than by an id; asserting it by an id needs seatd. That a
//! session refuses to ask twice at all is asserted here, on every backend.
//!
//! **Every device that is revoked.** A terminal switch leaves an evdev descriptor answering
//! `ENODEV`, and nothing puts it back; noop has no session to switch away from. What is asserted
//! below is the open, the give-back and what each costs.

// The environment is written once below, before this process has made a thread. The block states
// what makes it sound where it is made.
#![allow(unsafe_code)]

mod support;

use std::env;
use std::os::fd::{AsFd, AsRawFd};
use std::path::{Path, PathBuf};
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

    let nodes = support::openable_input_nodes();
    if nodes.is_empty() {
        eprintln!(
            "session_seated: no `event*` under /dev/input opens for this process, so where an \
             input device comes from was not asserted. The noop backend opens a node with a plain \
             `open(2)`, so this needs one this user may read and write: add this user to the \
             `input` group."
        );
        println!("session_seated: 6 checks passed");
        return;
    }

    println!("session_seated: an_input_device_comes_from_the_seat_and_goes_back_to_it");
    an_input_device_comes_from_the_seat_and_goes_back_to_it(&nodes);

    println!("session_seated: an_input_device_closed_and_opened_again_costs_what_it_did");
    an_input_device_closed_and_opened_again_costs_what_it_did(&nodes);

    println!("session_seated: a_path_this_session_holds_is_refused_a_second_open");
    a_path_this_session_holds_is_refused_a_second_open(&nodes);

    println!("session_seated: a_path_that_is_no_input_device_goes_back_to_the_seat");
    a_path_that_is_no_input_device_goes_back_to_the_seat(&nodes);

    println!("session_seated: every_input_device_goes_back_at_once");
    every_input_device_goes_back_at_once(&nodes);

    println!("session_seated: every_input_device_closed_and_opened_again_reads");
    every_input_device_closed_and_opened_again_reads(&nodes);

    if nodes.len() < 2 {
        eprintln!(
            "session_seated: only one `event*` under /dev/input opens for this process, so which \
             device a give-back names was not asserted. It needs two nodes this user may read and \
             write: add this user to the `input` group."
        );
        println!("session_seated: 12 checks passed");
        return;
    }

    println!("session_seated: one_input_device_goes_back_and_the_other_stays");
    one_input_device_goes_back_and_the_other_stays(&nodes);

    println!("session_seated: 13 checks passed");
}

/// One input device is opened through the seat, held by the session, and released through it.
///
/// The same handover the card gets, counted the same way: one node opened this way is **two**
/// descriptors, the one the seat opened and the duplicate the input device is built over. Letting
/// go of the device closes one of them, and only [`Session::close_input`] closes the other, so the
/// session keeps what it has to give back.
fn an_input_device_comes_from_the_seat_and_goes_back_to_it(nodes: &[PathBuf]) {
    let path = &nodes[0];
    let before = support::open_descriptors();
    let mut session = Session::open();
    let seated = support::open_descriptors();

    let mut device = session
        .open_input(path)
        .unwrap_or_else(|error| panic!("the noop backend opens any path it is given: {error}"));

    assert_eq!(
        device.path(),
        path,
        "the device names the node the seat opened"
    );
    assert!(
        !device.name().is_empty(),
        "and it was read, so it says what it is"
    );
    assert_eq!(
        support::open_descriptors(),
        seated + 2,
        "one input device is two descriptors: the one the seat opened and the duplicate the device \
         is built over"
    );
    // The node is opened non-blocking whatever the daemon handed over, so this answers rather than
    // waiting for somebody to touch the device.
    device.read().expect("a quiet device reads as nothing");

    drop(device);

    assert_eq!(
        support::open_descriptors(),
        seated + 1,
        "the caller let go of its own name on the node, and the seat's own device stays with the \
         session"
    );

    session.close_input(path);

    assert_eq!(
        support::open_descriptors(),
        seated,
        "the session gave the device back, which is the only thing that releases the daemon's \
         record of it"
    );

    session.close_input(path);
    assert_eq!(
        support::open_descriptors(),
        seated,
        "and a path this session holds nothing at is nothing to give back"
    );

    drop(session);

    assert_eq!(
        support::open_descriptors(),
        before,
        "the session went, and the seat with it"
    );
}

/// A node closed and opened again costs what it cost the first time.
///
/// This is the shape a resume needs and the shape `Seat::take_again` has: an evdev descriptor
/// another session took is revoked and nothing puts it back, so a device that comes back is a
/// device opened again. The close is made **first**, on the same path — seatd answers the same
/// device id with its reference count raised for a path the client already holds, so an open that
/// came first would get one id where it expected two.
///
/// Three rounds, because one open and one close prove nothing about the next: a session that kept
/// its record would carry the first round's cost into the second, and the count shows it did not.
fn an_input_device_closed_and_opened_again_costs_what_it_did(nodes: &[PathBuf]) {
    let path = &nodes[0];
    let mut session = Session::open();
    let seated = support::open_descriptors();

    for round in 0..3 {
        let mut device = session.open_input(path).unwrap_or_else(|error| {
            panic!(
                "round {round} asked the seat for {}: {error}",
                path.display()
            )
        });

        assert_eq!(
            support::open_descriptors(),
            seated + 2,
            "round {round} costs one node's two descriptors and nothing the round before left \
             behind"
        );
        device
            .read()
            .unwrap_or_else(|error| panic!("the device opened in round {round} reads: {error}"));

        // The descriptor closes here, and the session is told after it. See `Session::close_input`
        // for why that order is the one a daemon needs.
        drop(device);
        session.close_input(path);
    }

    assert_eq!(
        support::open_descriptors(),
        seated,
        "three rounds later the session holds what it held before the first one"
    );
}

/// A session holding two devices gives back the one it was asked for.
///
/// **This is the lookup [`Session::close_input`] makes, held to the path it was given.**
/// `Seat::open` walks about twenty nodes and gives back every one it declines, while holding the
/// two or three it took, so on a real machine that lookup runs against a list with a keyboard in
/// it. A lookup that answered the first entry whatever it was asked for would hand the keyboard
/// back on the first declined node after it — and logind revokes the device it is given back, under
/// a descriptor this run still holds and has grabbed, with nothing to put it back.
///
/// The **second** device is the one closed, because a lookup that ignores its path answers the
/// first.
fn one_input_device_goes_back_and_the_other_stays(nodes: &[PathBuf]) {
    let (first, second) = (&nodes[0], &nodes[1]);
    let mut session = Session::open();
    let seated = support::open_descriptors();

    let opened: Vec<zgui_evdev::Device> = [first, second]
        .into_iter()
        .map(|path| {
            session
                .open_input(path)
                .unwrap_or_else(|error| panic!("the seat opens {}: {error}", path.display()))
        })
        .collect();

    drop(opened);
    assert_eq!(
        support::descriptors_naming(first).len(),
        1,
        "the caller let go of its own name on {}, and the seat's own device stays with the session",
        first.display()
    );
    assert_eq!(
        support::descriptors_naming(second).len(),
        1,
        "and the same for {}",
        second.display()
    );

    session.close_input(second);

    assert!(
        support::descriptors_naming(second).is_empty(),
        "the device at the path the session was asked for went back"
    );
    assert_eq!(
        support::descriptors_naming(first).len(),
        1,
        "and the device it holds at another path is where it was: giving back {} took {} instead",
        second.display(),
        first.display()
    );
    assert_eq!(
        support::open_descriptors(),
        seated + 1,
        "so one of the two is still open"
    );

    session.close_input(first);

    assert_eq!(
        support::open_descriptors(),
        seated,
        "and the other goes back when it is asked for"
    );
}

/// A path this session already holds is refused, and nothing is opened for it.
///
/// One path is one device here, because that is what [`Session::close_input`] gives back. seatd
/// answers a path its client already holds with the *same* device id and its reference count
/// raised, so a second open would leave two records of one device, one give-back, and a daemon
/// holding the device for the rest of the run.
///
/// Nothing in this backend opens one path twice today. What this asserts is that a caller that
/// began doing it would be refused on every backend, rather than failing silently on seatd alone.
fn a_path_this_session_holds_is_refused_a_second_open(nodes: &[PathBuf]) {
    let path = &nodes[0];
    let mut session = Session::open();
    let seated = support::open_descriptors();

    let device = session
        .open_input(path)
        .unwrap_or_else(|error| panic!("the seat opens {}: {error}", path.display()));
    let refusal = session
        .open_input(path)
        .expect_err("a path this session holds a device at is refused a second device");

    assert!(
        refusal.to_string().contains(&path.display().to_string()),
        "the refusal names the path it was asked for, and it reads: {refusal}"
    );
    assert_eq!(
        support::open_descriptors(),
        seated + 2,
        "the second call asked the seat for nothing, so one node is still two descriptors"
    );

    drop(device);
    session.close_input(path);

    assert_eq!(
        support::open_descriptors(),
        seated,
        "and the one device the session took goes back in one call"
    );
}

/// Every device given back at once and every path opened again, three times over, and each one
/// reads.
///
/// This is a terminal switch seen from the session's side: the seat lets go of every device, gives
/// them all back in one call, and then walks the directory again. Each round is asserted to cost
/// what the first one cost, and each device that comes back is **read**, so the descriptor is known
/// to be live rather than merely counted.
///
/// What noop cannot show is the daemon's record. See what this binary leaves uncovered, above.
fn every_input_device_closed_and_opened_again_reads(nodes: &[PathBuf]) {
    let mut session = Session::open();
    let seated = support::open_descriptors();

    for round in 0..3 {
        let mut opened: Vec<zgui_evdev::Device> = nodes
            .iter()
            .map(|path| {
                session.open_input(path).unwrap_or_else(|error| {
                    panic!(
                        "round {round} asked the seat for {}: {error}",
                        path.display()
                    )
                })
            })
            .collect();

        assert_eq!(
            support::open_descriptors(),
            seated + 2 * nodes.len(),
            "round {round} costs every node's two descriptors and nothing the round before left \
             behind"
        );
        for device in &mut opened {
            let path = device.path().to_owned();
            device.read().unwrap_or_else(|error| {
                panic!(
                    "the device round {round} opened at {} answers a read: {error}",
                    path.display()
                )
            });
        }

        // The descriptors close here, and the session is told after them. See
        // `Session::close_every_input` for why that order is the one a daemon needs.
        drop(opened);
        session.close_every_input();

        assert_eq!(
            support::open_descriptors(),
            seated,
            "and round {round} gave all of them back in one call"
        );
    }
}

/// A path the seat opens that is no input device goes back, and the session says so.
///
/// A seat hands out graphics cards over the call it hands out input devices with, and the noop
/// backend opens any path at all, so this is an answer a real machine gives. The count proves the
/// device the seat opened went back rather than being kept.
fn a_path_that_is_no_input_device_goes_back_to_the_seat(nodes: &[PathBuf]) {
    let mut session = Session::open();
    let seated = support::open_descriptors();

    let refusal = session
        .open_input(Path::new(NOT_A_CARD))
        .expect_err("`/dev/null` answers no input request, so no device is built over it");

    assert!(
        refusal.to_string().contains(NOT_A_CARD),
        "the refusal names the path it was asked for, and it reads: {refusal}"
    );
    assert_eq!(
        support::open_descriptors(),
        seated,
        "the device the seat opened went back before the refusal was reported"
    );

    // The other direction, so that the check above is a check rather than a refusal of everything.
    let device = session
        .open_input(&nodes[0])
        .expect("a node the seat opens is taken");
    drop(device);
    session.close_input(&nodes[0]);
}

/// Every input device goes back in one call, which a suspend needs.
///
/// The devices are opened one at a time and given back together. The two counts below catch a
/// session that released only one of them, and one that released the card along with them.
fn every_input_device_goes_back_at_once(nodes: &[PathBuf]) {
    let mut session = Session::open();
    let card = session.card().expect("this machine has a card to open");
    let taken = support::open_descriptors();

    let opened: Vec<zgui_evdev::Device> = nodes
        .iter()
        .map(|path| {
            session
                .open_input(path)
                .unwrap_or_else(|error| panic!("the seat opens {}: {error}", path.display()))
        })
        .collect();

    assert_eq!(
        support::open_descriptors(),
        taken + 2 * nodes.len(),
        "every node cost the seat's own descriptor and a duplicate"
    );

    drop(opened);
    session.close_every_input();

    assert_eq!(
        support::open_descriptors(),
        taken,
        "and all of them went back in one call"
    );
    assert!(
        session.card().is_ok(),
        "the card is held apart from the input devices, so giving them back leaves it where it is"
    );
    assert_eq!(
        support::descriptors_naming(card.path()).len(),
        2,
        "and it still costs the two descriptors it did"
    );
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
