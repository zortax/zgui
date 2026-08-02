//! An open window with a graphics device behind it, and the pixels it actually drew.
//!
//! [`desktop`](crate::desktop) drives the whole framework and draws nowhere; [`device`](crate::device)
//! draws with the real renderer and reads the target back. Neither on its own can answer *does this
//! control look different while the pointer is on it*, because that question needs a pointer stream
//! and a picture at the same time. This is the two put together, and it is the only shape in which
//! the appearance of an interaction can be measured at all.
//!
//! | Module | Contents |
//! |---|---|
//! | [`stage`] | the window, the pointer, the clock and the readback |
//! | [`words`] | whether a run of words was drawn, and where its letters landed |

#![allow(
    dead_code,
    unreachable_pub,
    reason = "one support module serves several groups of assertions, none of which uses all of it"
)]

pub mod stage;
pub mod words;
