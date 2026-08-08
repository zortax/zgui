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
//! * **No session or virtual terminal management.** Nothing here takes DRM master, hands the
//!   device back on a terminal switch, or asks a session daemon for it. `Output::discover` reads
//!   the device and takes no master; `Scanout` expects the caller to hold master already. So a
//!   caller needs a free virtual terminal, or root, and is refused while a compositor holds the
//!   master.
//!
//! The crate's dependencies say the same: no input crate, and no session library.
//!
//! # How a frame reaches the screen
//!
//! `Scanout` owns two buffers per display and flips between them. A frame the renderer read back
//! is copied into whichever is off screen, the flip is asked for, and the completion event says
//! the frame arrived and the other buffer is free again. The copy chooses its fourcc from the
//! channel order of the readback, so a frame is copied rather than swizzled pixel by pixel.
//!
//! # The handles a surface reports
//!
//! A `DrmSurface` carries the native handles a KMS display has: the device descriptor as
//! `DrmDisplayHandle`, and the primary plane as `DrmWindowHandle`. They are the only route to this
//! backend's native state, so a DRM-aware renderer written outside this workspace reaches it
//! through the platform contract and needs no fork of the backend.
//!
//! No graphics API in this workspace's dependency set reads those two variants yet. wgpu answers a
//! DRM handle with "not a Vulkan-compatible handle", which is a true report of where the gap is. A
//! renderer that draws through this backend reaches an application through `App::with_renderer`.

// Every item this doc names is Linux-only and the doc itself is not, so the names above are in
// backticks rather than intra-doc links. A link to a `cfg`-gated item is broken on every other
// platform, and this workspace denies a broken one.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for exactly one reason: a surface hands out its
// native handles, and `raw-window-handle`'s `borrow_raw` constructors are unsafe. Every unsafe
// block states what makes it sound. This backend still issues no ioctl of its own — `zgui-drm`
// owns every one of them.
#![allow(unsafe_code)]

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
pub mod scanout;
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
pub use crate::scanout::Scanout;
#[cfg(target_os = "linux")]
pub use crate::surface::DrmSurface;
#[cfg(target_os = "linux")]
pub use crate::waker::EventfdWaker;
