//! Input policy, as libinput applies it.
//!
//! A device reports which key moved and how far a wheel turned. What a person did is a further
//! question: how far a pointer travels for one movement, whether a tap on a touchpad is a click,
//! whether two fingers are a scroll, whether a palm on the pad is anything, and whether a worn
//! switch that reports one press as three is trusted. libinput answers all of it. It carries the
//! acceleration curves, the touchpad state machines and a database of quirks for named hardware,
//! and every desktop on Linux asks it. This crate is how it is asked.
//!
//! The crate names no zgui crate and is usable on its own. It is written to be named by
//! `zgui-platform-drm`, one step along the same layer, and by nothing else.
//!
//! # Loading
//!
//! libinput is loaded at run time with `dlopen`. A build needs neither the library nor a device,
//! and a machine that has neither still starts: the absence arrives as [`Error::Library`], which a
//! caller reads and answers. A console session answers it by reading the devices itself, and does
//! without everything above.
//!
//! ```
//! use zgui_libinput::{Error, Library};
//!
//! match Library::load() {
//!     Ok(_library) => println!("libinput is on this machine"),
//!     Err(Error::Library { tried, reason }) => println!("none of {tried:?} opened: {reason}"),
//!     Err(other) => println!("{other}"),
//! }
//! ```
//!
//! # Device access
//!
//! libinput opens nothing. It asks its caller for a descriptor and hands it back when it is done,
//! so a session daemon can own a device while libinput reads it. The same seam gives a device up
//! for a terminal switch and takes it again afterwards.
//!
//! libinput takes no exclusive grab. A caller that needs one makes it on the descriptor it hands
//! over, as `libinput debug-events --grab` does.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for one reason: libinput is opened at run time and
// called through the addresses that come back, so every entry point is an FFI call through a
// resolved symbol. Every unsafe block states what makes it sound.
#![allow(unsafe_code)]

pub mod error;
pub mod library;

pub use crate::error::{Error, Result};
pub use crate::library::Library;
