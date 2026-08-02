//! Proof that both intended implementations of this contract can actually be written.
//!
//! A trait boundary is only a boundary if more than one thing fits behind it, and a boundary that
//! has been designed against exactly one implementation always fits that one. Two implementations
//! are planned here and they are as different as the contract has to tolerate:
//!
//! * a **windowing backend**, driving a real event loop with real windows, a real clipboard, real
//!   accessibility adapters and a real graphics device; and
//! * a **headless backend**, with a clock that only moves when a test moves it, input that is
//!   scripted rather than received, a surface that is a buffer, and no windowing system at all.
//!
//! Neither of them lives in this crate — the whole point is that this crate names neither a
//! windowing library nor a graphics API. What lives here is the check that each is expressible:
//! the headless one is written out in full and driven through the contract, and the parts of the
//! windowing one that constrain the *shape* of the traits — thread safety, usability behind a
//! pointer, the graphics handles — are asserted at compile time.
//!
//! Nothing here is reachable from outside the crate, and it must stay that way. A shipped headless
//! backend is a crate of its own; the moment one exists, this module is a *second* implementation
//! of the same thing, and the one that a test picked up by accident would be the one nobody
//! maintains. It is a proof, and it is deleted rather than promoted.

mod headless;
mod windowing;
