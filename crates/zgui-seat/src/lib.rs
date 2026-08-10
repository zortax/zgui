//! The session, as libseat opens it.
//!
//! A program that draws on a console opens a graphics card and reads the keyboards and the mice.
//! Every one of those devices belongs to the session that owns the screen. A program that opens
//! them itself needs root and a terminal reserved in advance, and it holds them until it exits.
//! libseat asks the session daemon instead: the daemon opens each device, hands the descriptor
//! over, takes the devices back when a person switches to another terminal, and gives them back on
//! the way in. This crate is how libseat is asked.
//!
//! The crate names no other zgui crate and is usable on its own. It is written to be named by
//! `zgui-platform-drm`, one step along the same layer, and by nothing else.
//!
//! # Loading libseat
//!
//! libseat is loaded at run time with `dlopen`. A build therefore needs neither the library nor a
//! session daemon, and a machine that has neither still starts: the absence arrives as
//! [`Error::Library`], which a caller reads and answers. A console session answers it by opening
//! the devices itself, and pays the privilege that costs.
//!
//! [`Library::load`] is that same open on its own, for a caller that wants to know whether libseat
//! is here before it asks for a seat. The addresses live inside [`Library`] and are reached through
//! it, so the mapping stands for as long as anything can call one. The table itself is internal.
//!
//! # Portability
//!
//! `zgui-drm` and `zgui-evdev` are gated on Linux, because every line of them is a call into the
//! kernel. This one links nothing and resolves its symbols at run time, so a machine with no
//! libseat and no session daemon still builds it. A build needs a unix host, because
//! [`Seat::descriptor`] hands out a `BorrowedFd`, which the standard library has on unix.
//!
//! # Backends
//!
//! libseat picks a backend when a seat is opened, and the machine settles which one: logind where
//! it runs the terminals, seatd where a daemon listens on a socket, and a builtin backend that
//! needs root. Whether any of them answers is settled the same way, so a caller reads what it got.
//!
//! # Using a seat
//!
//! [`Seat::open`] opens the library, opens the seat this session is on, and waits for it to become
//! usable. [`Seat::descriptor`] is what a loop waits on, and [`Seat::dispatch`] answers a list of
//! [`Change`]s. Dropping the seat closes it and gives the terminal back.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for one reason: libseat is opened at run time and
// called through the addresses that come back, so every entry point is an FFI call through a
// resolved symbol. Every unsafe block states what makes it sound.
#![allow(unsafe_code)]

pub mod error;
pub mod library;
pub mod seat;

pub use crate::error::{Error, Result};
pub use crate::library::Library;
pub use crate::seat::{Change, ENABLE_WITHIN, Seat};
