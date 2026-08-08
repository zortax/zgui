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
//! * **No session or virtual terminal management.** This backend will take DRM master and hold it
//!   for as long as the program runs. Nothing will hand the device back on a terminal switch, and
//!   nothing asks a session daemon for the device. So a program here will need a free virtual
//!   terminal, or root, and will fail to start while a compositor holds the master.
//!
//! The crate's dependencies say the same: no input crate, and no session library.

#![deny(missing_docs)]
// This backend issues no ioctl of its own — `zgui-drm` owns every one of them — and the pixels it
// moves are slices. Forbidding unsafe is the claim that this stays true.
#![forbid(unsafe_code)]
