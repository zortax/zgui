//! Turning what the kernel reports into what a document dispatches.
//!
//! Every translation here narrows: a device reports which key moved, and a document is told what a
//! person typed. The two vocabularies are different, and that difference is what a program above
//! this backend must not have to know about. It is written down once, here, and checked.
//!
//! Four narrowings carry the weight, and each has its own module:
//!
//! * a key press answers three questions with three answers — what to insert, which shortcut it
//!   is, and which physical position was pressed — and all three travel together ([`keyboard`]);
//! * a device says how far it moved or where it is, and a document is told where the pointer now
//!   is. The position between those reports belongs to this backend, and so does the ground it
//!   moves over ([`mod@pointer`]);
//! * a scroll keeps the unit the device measures in. Detents cross to pixels through a line
//!   height that only the element being scrolled knows, and a wheel that reports both units
//!   reports one movement in both, so one unit is dropped ([`wheel`]);
//! * a device is a descriptor, a grab and a stream of batches, and what a person did with one
//!   belongs to a surface ([`seat`]).
//!
//! # The layout
//!
//! A window system answers "what does this key mean" for a backend that has one. A console has
//! nothing to ask, so this backend asks a layout itself: libxkbcommon where the machine has it,
//! and the keymap the kernel's own console driver holds elsewhere. The two express different
//! things, so which one a program got is reported at start-up. See [`keyboard::layout`].
//!
//! # Scope
//!
//! The keyboard and the pointer, including the ones plugged in while the program runs. The set of
//! devices is read at start-up and added to as devices arrive, and one that stops answering is
//! dropped. What a cursor looks like is [`crate::cursor`], because a picture is no translation.
//!
//! A second pointer and a touch protocol are absent. Every device drives the one pointer, so two
//! fingers on a touchscreen are one pointer that jumps between them. The crate documentation lists
//! that beside the rest of what this backend does not have.

pub mod keyboard;
pub(crate) mod lent;
pub mod pointer;
pub mod seat;
pub(crate) mod through;
pub mod wheel;

#[cfg(test)]
pub(crate) mod fixture {
    //! What the modules beside this one build a record out of.
    //!
    //! Three of them write `input_event` bytes for a reader to walk, and the time at the front of
    //! one is the single part of that record whose width follows the target. It is written here
    //! once so that the three agree.

    /// The bytes of the time one record is stamped with, at this target's width.
    ///
    /// The seconds and the microseconds are one word each: eight bytes where a record is
    /// twenty-four and four where it is sixteen. `zgui_evdev`'s generated interface states which
    /// target gets which, and why both carry the same value.
    pub(crate) fn stamp(seconds: i64, microseconds: i64) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16);
        if cfg!(target_pointer_width = "64") {
            bytes.extend_from_slice(&seconds.to_ne_bytes());
            bytes.extend_from_slice(&microseconds.to_ne_bytes());
        } else {
            let seconds = i32::try_from(seconds).expect("a test uses a small moment");
            let microseconds = i32::try_from(microseconds).expect("a microsecond count fits");
            bytes.extend_from_slice(&seconds.to_ne_bytes());
            bytes.extend_from_slice(&microseconds.to_ne_bytes());
        }
        bytes
    }
}
