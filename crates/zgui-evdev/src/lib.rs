//! The kernel's input interface, read through the uapi headers.
//!
//! This is what libevdev is, written in Rust. It links no C. Every call into the kernel is an
//! ioctl on a file descriptor, issued through `rustix`'s `linux_raw` backend.
//!
//! Nothing here knows what zgui is. The crate is usable on its own, and it is tested on its own:
//! the request numbers and the sizes they are computed from are asserted against what the kernel's
//! own headers hold, with no device present.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for one reason: every call into the kernel is an
// ioctl, and an event record is read out of a byte buffer. Every unsafe block states why it is
// sound.
#![allow(unsafe_code)]

// Every module in this crate is the kernel's interface or something built directly on it, so on
// any other platform the crate holds nothing at all. Each module is gated and the crate itself is
// not, so `cargo check --workspace` passes on a machine this code could never run on.
#[cfg(target_os = "linux")]
pub mod error;
#[cfg(target_os = "linux")]
mod ioctl;
#[cfg(target_os = "linux")]
mod sys;

#[cfg(target_os = "linux")]
pub use crate::error::{Error, Result};
