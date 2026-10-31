//! Copying between the buffers a display scans out of, on the device that owns them.
//!
//! A program that presents through a rotating set of buffers owes each one the damage it missed
//! while it was not the one being written. Those pixels are already correct in whichever buffer was
//! presented last, so they need not be produced again — and where the renderer is on a *different*
//! device from the display, producing them again means sending them across the link a second time.
//! This crate copies them where they already are.
//!
//! It is worth having only where those two devices differ. On one device the copy costs what the
//! draw costs and the whole exercise is a wash. On a machine whose link is narrow it is the
//! difference between sending a frame's damage and sending several frames of it: the machine this
//! was written for has a single PCIe lane, where a copy runs about five times the rate the link
//! writes at.
//!
//! # What it does not decide
//!
//! Which buffer to draw into, how old each one is, and which rectangles each is owed. That is
//! arithmetic over the set and it needs no device, so it belongs to whoever presents. This crate
//! answers one question — *copy these rectangles from that buffer to this one* — and answers it on
//! the device the buffers live on.
//!
//! # Who waits
//!
//! [`Copier::begin`] starts a copy and waits for nothing, and [`Copier::finish`] is the wait —
//! nothing may read the buffer written to until that has answered. A device that exported a fence
//! descriptor could let the **kernel** carry the wait instead, by handing it to whatever the caller
//! commits next. The hardware this was written against exports none, so the wait happens here.
//!
//! # Loading
//!
//! Everything is opened at run time. A build needs neither EGL nor libgbm nor a card, and a machine
//! that has none of them still starts: the absence arrives as an [`Error`], which a caller reads and
//! answers by sending the pixels the long way round, exactly as it did before.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for one reason: EGL and libgbm are opened at run
// time and every call into either is a call through a pointer this crate resolved itself. There is
// no safe spelling of that, and the alternative — linking them — is what stops the console session
// starting on a machine that has neither.
#![allow(unsafe_code)]

// Every module here reaches EGL or libgbm over a file descriptor, and `std::os::fd` exists only on
// a Unix host, so on any other platform this crate is empty and `cargo check --workspace` passes on
// a machine this code could never run on. The same arrangement `zgui-drm` and `zgui-evdev` keep.
#[cfg(target_os = "linux")]
mod buffer;
#[cfg(target_os = "linux")]
mod copier;
#[cfg(target_os = "linux")]
mod error;

#[cfg(target_os = "linux")]
pub mod egl;

#[cfg(target_os = "linux")]
pub use crate::buffer::{Buffer, Rect};
#[cfg(target_os = "linux")]
pub use crate::copier::Copier;
#[cfg(target_os = "linux")]
pub use crate::error::Error;
