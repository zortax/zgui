//! The platform contract on a raw Linux console: a display, a mode, and a page flip.
//!
//! This is the platform backend for a machine with no display server. It drives `zgui-drm`
//! directly, so the picture a program presents goes to the screen through the kernel and through
//! nothing else. A user interface written against the contract runs here unchanged.
//!
//! # What this does not have yet
//!
//! * **No input.** Reading the evdev devices is a sub-project of its own, and it is not started.
//!   An application on this backend draws and animates, and a person cannot touch it: no keyboard,
//!   no pointer, no touch.
//! * **No session or virtual terminal management.** The frame loop will take DRM master and hold
//!   it for as long as the program runs. Nothing will hand the device back on a terminal switch,
//!   and nothing asks a session daemon for it. So a program here will need a free virtual
//!   terminal, or root, and will fail to start while a compositor holds the master.
//!
//! Both are visible in what the crate names: no input crate, and no session library.
//!
//! Read that second one as a description of the loop this crate is growing rather than of the code
//! in it today. [`Output::discover`](crate::output::Output::discover) exists now, and it reads
//! the device without taking master at all.

#![deny(missing_docs)]
// This backend issues no ioctl of its own — `zgui-drm` owns every one of them — and the pixels it
// moves are slices. Forbidding unsafe is the claim that this stays true.
#![forbid(unsafe_code)]

// The kernel's display interface exists on Linux and nowhere else, so on any other platform this
// crate is empty rather than broken.
#[cfg(target_os = "linux")]
pub mod clipboard;
#[cfg(target_os = "linux")]
pub mod clock;
#[cfg(target_os = "linux")]
pub mod cx;
#[cfg(target_os = "linux")]
pub mod output;
#[cfg(target_os = "linux")]
pub mod surface;
#[cfg(target_os = "linux")]
pub mod waker;

#[cfg(target_os = "linux")]
pub use crate::clipboard::ConsoleClipboard;
#[cfg(target_os = "linux")]
pub use crate::clock::SystemClock;
#[cfg(target_os = "linux")]
pub use crate::cx::DrmCx;
#[cfg(target_os = "linux")]
pub use crate::output::Output;
#[cfg(target_os = "linux")]
pub use crate::surface::DrmSurface;
#[cfg(target_os = "linux")]
pub use crate::waker::EventfdWaker;
