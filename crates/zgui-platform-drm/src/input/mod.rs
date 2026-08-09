//! Turning what the kernel reports into what a document dispatches.
//!
//! Every translation here narrows: a device reports which key moved, and a document is told what a
//! person typed. The two vocabularies are different, and that difference is what a program above
//! this backend must not have to know about. It is written down once, here, and checked.
//!
//! Two narrowings carry the weight, and each has its own module:
//!
//! * a key press answers three questions with three answers — what to insert, which shortcut it
//!   is, and which physical position was pressed — and all three travel together ([`keyboard`]);
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
//! The keyboard, and nothing else. The pointer, the shape a cursor is drawn with, and a device
//! plugged in while the program runs are later work: no mouse is read here, and the set of devices
//! is read once.

pub mod keyboard;
pub mod pointer;
pub mod seat;
pub mod wheel;
