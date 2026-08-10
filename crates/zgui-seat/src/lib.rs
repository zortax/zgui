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
//! [`Library::load`] opens the shared object and resolves every symbol this crate calls.
//! [`Library::symbols`] lends out the addresses, and the borrow holds the mapping open for as long
//! as anything can reach into it.
//!
//! # Portability
//!
//! `zgui-drm` and `zgui-evdev` are gated on Linux, because every line of them is a call into the
//! kernel. This one links nothing and resolves its symbols at run time, so it compiles on any host.
//!
//! # Backends
//!
//! libseat picks a backend when a seat is opened, and the machine settles which one: logind where
//! it runs the terminals, seatd where a daemon listens on a socket, and a builtin backend that
//! needs root. Whether any of them answers is settled the same way, so a caller reads what it got.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for one reason: libseat is opened at run time and
// called through the addresses that come back, so every entry point is an FFI call through a
// resolved symbol. Every unsafe block states what makes it sound.
#![allow(unsafe_code)]

pub mod error;
pub mod library;

pub use crate::error::{Error, Result};
pub use crate::library::Library;
