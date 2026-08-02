//! The phases that compare two pictures of one state rather than timing one.
//!
//! A differential mounts the same document twice — once driven incrementally, once repainted whole
//! — walks the 42-step script over both, and asks whether they agree. None of them is a
//! measurement: each is an assertion with a number attached to it, and each exits non-zero when the
//! two pictures differed.
//!
//! | Phase | What it compares |
//! |---|---|
//! | [`theme`] | the display list either side of a colour-scheme flip |
//! | [`transcript`] | the finished display list as text, live against a full repaint |
//! | [`pixels`] | what was actually drawn |
//! | [`hits`] | what is under a point |
//! | [`a11y`] | the rectangles a consumer outside the process is given |
//!
//! The last two compare things that are *not drawn*. Everything a display list and a set of
//! rectangles can settle is settled by the first three; an element hit where it is not, and a
//! control reported to a screen magnifier where it is not, are invisible to all of them.

mod a11y;
mod hits;
mod pixels;
mod theme;
mod transcript;
mod twin;

use crate::phase::Driver;

/// Runs one of this group's phases, or answers `None` when the name is not one of them.
pub(crate) fn run(driver: &mut Driver, phase: &str) -> Option<u64> {
    theme::run(driver, phase)
        .or_else(|| transcript::run(driver, phase))
        .or_else(|| pixels::run(driver, phase))
        .or_else(|| hits::run(driver, phase))
        .or_else(|| a11y::run(driver, phase))
}
