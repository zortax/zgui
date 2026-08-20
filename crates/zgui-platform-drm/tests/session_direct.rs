//! What a session answers where libseat cannot be opened, and what the direct path then takes.
//!
//! The library is opened at run time, so a machine that has none is the ordinary case rather than a
//! broken one, and the answer is the direct shape: this process opens the card and takes DRM master
//! itself, as this backend did before there was a session at all.
//!
//! # Putting the seat out of reach
//!
//! Asserting the fallback against whatever this machine happens to have would assert nothing where
//! libseat is installed, and nothing anybody can rely on where it is absent. So the seat is put out
//! of reach instead: the process holds a descriptor limit of zero for the length of the open, and
//! every step towards a seat wants a descriptor. The loader opens no shared object, so every soname
//! answers "too many open files"; and libseat's noop backend, which is the one `LIBSEAT_BACKEND`
//! names here, makes a `socketpair` for the seat it would otherwise answer with. Either way
//! `Session::open` is handed a failure, so a fallback that stopped happening fails this binary
//! rather than leaving it to answer whatever this machine would have answered anyway.
//!
//! # The order of the two halves
//!
//! The subject runs **first**, and the machine is asked about libseat **afterwards**. The probe
//! opens the library and closes it again: glibc unmaps a shared object when the last handle on it
//! closes, so a probe that ran first would leave nothing behind there. musl unmaps nothing, and a
//! process that links libseat directly has it mapped from the start — and the loader answers a name
//! it has already mapped out of its own list without opening anything. So on either of those a
//! probe that ran first would leave libseat in this process and the descriptor limit would stop
//! nothing. The order costs nothing and holds everywhere.
//!
//! # What this binary does not cover
//!
//! **That a session ever comes back seated.** A `Session::open` that answered the direct shape on
//! every machine passes everything here, and `session_seated` is what refuses it.
//!
//! **The direct path over a real card.** Opening one and taking DRM master on it needs root or a
//! virtual terminal nothing else holds, and it blanks the console it succeeds on. So what is
//! asserted below is that the master is asked for and that a refusal reaches the caller, over a
//! path that opens for anybody and answers no DRM request. A card, a mode and a picture are read on
//! hardware.
//!
//! **`run`.** The frame loop takes a card, lights every display it finds and turns until the
//! application stops, so nothing here can call it. The console and the master it needs live in
//! `Session`, which takes both together and gives both back in its own `Drop`.

// This binary has its own `main`, so nothing here runs on a thread the harness made. The descriptor
// limit and the environment both belong to the process, and either one written while another test
// read it would fail that test rather than this one.
#![allow(unsafe_code)]

mod support;

use std::env;
use std::path::PathBuf;

use zgui_platform_drm::Session;

/// The variable libseat reads the backend name out of.
const BACKEND: &str = "LIBSEAT_BACKEND";

/// A path every user opens and no driver answers a DRM request on.
const NOT_A_CARD: &str = "/dev/null";

/// Opens a session with the seat out of reach, and reports what it got.
fn main() {
    // SAFETY: this is the first statement of the process. No thread has been made yet, so nothing
    // else can read the environment while it is written, and libseat is reached for below, which is
    // after this returns.
    unsafe { env::set_var(BACKEND, "noop") };

    // Before the probe below, which would map libseat into this process. See the module
    // documentation.
    let mut session = support::while_nothing_opens(Session::open);

    if !support::libseat_is_installed() {
        eprintln!(
            "session_direct: this machine has no libseat, so the fallback was asserted over a \
             machine that has nothing to fall back from and nothing was covered. Install seatd, or \
             run the suite from `nix develop`, which puts `libseat.so.1` on the library path."
        );
        return;
    }

    println!("session_direct: a session with no seat opens the devices itself");
    assert!(
        !session.is_seated(),
        "no seat could be opened, so no daemon holds the devices and this run takes them itself"
    );

    println!("session_direct: the direct path takes DRM master, and stops where it cannot");
    the_direct_path_takes_the_master(&mut session);

    println!("session_direct: 2 checks passed");
}

/// The direct path asks for DRM master on the card it opened, and reports a refusal.
///
/// Master is the interlock. A machine where a compositor holds the display refuses it, and the run
/// stops there — before the console is blanked and before the keyboard is taken away from the
/// desktop that is using it. So the walk is pointed at a path that opens for any user and answers
/// no DRM request: what comes back says the master was asked for at all.
fn the_direct_path_takes_the_master(session: &mut Session) {
    let refusal = session
        .card_from(&[PathBuf::from(NOT_A_CARD)])
        .expect_err("`/dev/null` opens and refuses `SET_MASTER`, so no card comes back from it");

    assert!(
        refusal.to_string().contains("DRM master"),
        "the refusal says which step this run stopped at, and it reads: {refusal}"
    );
}
