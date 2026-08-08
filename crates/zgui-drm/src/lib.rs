//! The kernel's display interface: modes, planes, framebuffers and page flips.
//!
//! A Rust implementation of the DRM uapi, written against the vendored headers. It links no C.
//! Every call is an ioctl on a file descriptor, issued through `rustix`'s `linux_raw` backend.
//!
//! Nothing here knows what zgui is. The crate is usable on its own, and it is tested on its own
//! against `vkms`, the kernel's virtual display driver, so the whole of it runs with no hardware.
//!
//! # The two modesetting interfaces
//!
//! The atomic interface describes a whole display configuration as a set of object properties and
//! applies it in one call. Such a call can be tested before it is applied, and it can carry a
//! fence. The legacy interface sets a CRTC and flips a page, and does neither.
//!
//! Which one a device uses is decided when it is opened: by whether the kernel accepts
//! `DRM_CLIENT_CAP_ATOMIC`, and by which interface the caller asked for. `Device::is_atomic`
//! reports the answer.
//!
//! A caller may ask for the legacy interface on a device that has both. Every atomic driver still
//! serves the legacy ioctls, because the kernel implements them over its own atomic helpers, so
//! asking for the legacy interface is how that path is exercised on hardware.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for two reasons: every call into the kernel is an
// ioctl, and a dumb buffer is reached through a mapping. Every unsafe block states why it is sound.
#![allow(unsafe_code)]

// Every module in this crate is the kernel's interface or something built directly on it, so on
// any other platform the crate is empty. `cargo check --workspace` then passes on a machine this
// code could never run on.
#[cfg(target_os = "linux")]
pub mod buffer;
#[cfg(target_os = "linux")]
pub mod commit;
#[cfg(target_os = "linux")]
pub mod device;
#[cfg(target_os = "linux")]
pub mod error;
#[cfg(target_os = "linux")]
pub mod format;
#[cfg(target_os = "linux")]
pub mod framebuffer;
#[cfg(target_os = "linux")]
mod ioctl;
#[cfg(target_os = "linux")]
pub mod property;
#[cfg(target_os = "linux")]
pub mod resources;
#[cfg(target_os = "linux")]
mod sys;

#[cfg(target_os = "linux")]
pub use crate::commit::Commit;
#[cfg(target_os = "linux")]
pub use crate::device::Device;
#[cfg(target_os = "linux")]
pub use crate::error::{Error, Result};
