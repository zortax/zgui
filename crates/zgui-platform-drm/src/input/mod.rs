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
//! The keyboard and the pointer. What a cursor looks like is [`crate::cursor`], because a picture
//! is no translation. A device plugged in while the program runs is later work: the set of devices
//! is read once, so a mouse plugged in afterwards reaches nothing.

pub mod keyboard;
pub mod pointer;
pub mod seat;
pub mod wheel;
