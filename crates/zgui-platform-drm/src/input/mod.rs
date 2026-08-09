//! Turning what the kernel reports into what a document dispatches.
//!
//! Every translation here narrows: a device reports which key moved, and a document is told what a
//! person typed. The two vocabularies are different, and that difference is what a program above
//! this backend must not have to know about. It is written down once, here, and checked.
//!
//! One narrowing carries the weight and has a module of its own: a key press answers three
//! questions with three answers — what to insert, which shortcut it is, and which physical
//! position was pressed — and all three travel together ([`keyboard`]).
//!
//! # Scope
//!
//! The keyboard, and nothing else. The pointer, the shape a cursor is drawn with, and a device
//! plugged in while the program runs are later work.

pub mod keyboard;
