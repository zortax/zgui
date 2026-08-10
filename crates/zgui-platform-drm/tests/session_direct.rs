//! What a session answers where libseat cannot be opened.
//!
//! The library is opened at run time, so a machine that has none is the ordinary case rather than a
//! broken one, and the answer is the direct shape: this process opens the card and takes DRM master
//! itself, as this backend did before there was a session at all.
//!
//! # Putting the library out of reach
//!
//! Asserting the fallback against whatever this machine happens to have would assert nothing where
//! libseat is installed, and nothing anybody can rely on where it is absent. So the library is put
//! out of reach instead: the process holds a descriptor limit of zero for the length of the open,
//! the loader can open no shared object at all, and every soname answers "too many open files".
//!
//! `LIBSEAT_BACKEND` names the noop backend all the same, which is the backend that opens a seat on
//! any machine and asks nothing of it. So the fallback is asserted over a machine that would
//! otherwise hand a seat back, and a fallback that stopped happening fails this binary rather than
//! leaving it to answer whatever this machine would have answered anyway.
//!
//! # The order of the two halves
//!
//! The subject runs **first**, and the machine is asked about libseat **afterwards**. The loader
//! answers a name it has already mapped without opening anything, so a probe that ran first would
//! leave libseat in this process and the descriptor limit would stop nothing.
//!
//! # What this binary does not cover
//!
//! That a session ever comes back seated. A `Session::open` that answered the direct shape on every
//! machine passes everything here, and `session_seated` refuses it.

// This binary has its own `main`, so nothing here runs on a thread the harness made. The descriptor
// limit and the environment both belong to the process, and either one written while another test
// read it would fail that test rather than this one.
#![allow(unsafe_code)]

mod support;

use std::env;

use zgui_platform_drm::Session;

/// The variable libseat reads the backend name out of.
const BACKEND: &str = "LIBSEAT_BACKEND";

/// Opens a session with the library out of reach, and reports what it got.
fn main() {
    // SAFETY: this is the first statement of the process. No thread has been made yet, so nothing
    // else can read the environment while it is written, and libseat is reached for below, which is
    // after this returns.
    unsafe { env::set_var(BACKEND, "noop") };

    // Before the probe below, which would map libseat into this process. See the module
    // documentation.
    let session = support::while_nothing_opens(Session::open);

    if !support::libseat_is_installed() {
        eprintln!(
            "session_direct: this machine has no libseat, so the fallback was asserted over a \
             machine that has nothing to fall back from and nothing was covered. Install seatd, or \
             run the suite from `nix develop`, which puts `libseat.so.1` on the library path."
        );
        return;
    }

    println!("session_direct: a session with no library takes the console and the master itself");
    assert!(
        session.takes_the_console(),
        "libseat could not be opened, so nothing else has put the terminal into graphics mode and \
         this session does it"
    );
    assert!(
        session.takes_the_master(),
        "libseat could not be opened, so no daemon granted master and this session takes it, and \
         owes it back"
    );

    println!("session_direct: 1 check passed");
}
