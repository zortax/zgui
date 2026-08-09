//! The platform contract on a raw Linux console: a display, a mode, and a page flip.
//!
//! This is the platform backend for a machine with no display server. It drives `zgui-drm`
//! directly, so the picture a program presents goes to the screen through the kernel and through
//! nothing else. A user interface written against the contract runs here unchanged.
//!
//! # Input
//!
//! **A person can type into an application here.** The frame loop opens every device somebody
//! could type on, takes each one away from everything else, and turns what the kernel reports into
//! the events a document is dispatched. What a key *means* comes from libxkbcommon where the
//! machine has it and from the kernel's own console keymap where it does not, and which of the two
//! a program got is stated once at start-up. Dead keys and compose sequences are applied where
//! libxkbcommon has the data for them, so `´` then `e` inserts `é`. `input` holds that
//! translation, all of it.
//!
//! A grabbed keyboard costs one thing worth knowing before running anything here: `Ctrl+C` never
//! reaches the terminal's line discipline, so no `SIGINT` is raised. This backend invents no quit
//! key — which key leaves a program is the program's own decision — so an application that binds
//! none has to be killed from another terminal.
//!
//! # What this does not have yet
//!
//! * **No pointer and no touch.** Only the keyboard is read. A mouse's own node is opened by
//!   nothing here, there is no cursor to give a shape to, and a device plugged in while the
//!   program runs is found by nothing: the set of devices is read once.
//! * **No input method.** Dead keys and compose sequences work, because libxkbcommon carries them
//!   and this backend feeds them. What is absent is an input method with a candidate window, so a
//!   Japanese or a Chinese keyboard types no more here than its Latin keys and
//!   `DrmSurface::set_text_input` does nothing.
//! * **No session or virtual terminal management.** The frame loop takes DRM master and holds it
//!   for as long as the program runs. Nothing hands the device back on a terminal switch, and
//!   nothing asks a session daemon for it. So a program here needs a free virtual terminal, or
//!   root, and fails to start while a compositor holds the master.
//!
//! The crate's dependencies say the same: no session library.
//!
//! # How a frame reaches the screen
//!
//! `Scanout` owns two buffers per display and flips between them. A frame the renderer read back
//! is copied into whichever is off screen, the flip is asked for, and the completion event says
//! the frame arrived and the other buffer is free again. The copy chooses its fourcc from the
//! channel order of the readback, so a frame is copied rather than swizzled pixel by pixel.
//!
//! A window system presents for a caller; a console does not. So the last step belongs to whatever
//! draws, and this crate offers the three things that step needs: `FORMAT`, the texture a frame is
//! composed into; `DrmDisplay::present`, which copies a composed frame into the buffer a display is
//! about to scan out of and asks for the flip; and `Displays`, which says which display a surface
//! is. The renderer that uses them lives in `zgui`, because a renderer is built by the runtime and
//! a backend at this layer cannot name the runtime.
//!
//! # The loop
//!
//! `run` is the driver. It opens the device, takes master, lights every display it finds, and then
//! turns: read the completions, read what the keyboards reported, draw the frames that were asked
//! for, ask the application how to wait, and wait on the device, the wake channel and every
//! keyboard together. `park` is where the waiting is decided, and it is the same state machine the
//! windowing backend parks with.
//!
//! It also writes the displays it lit into the `Displays` it was given, for as long as it turns.
//! That map and the renderer are one decision, so `App::run_drm` makes one map and hands it to
//! both.
//!
//! # The handles a surface reports
//!
//! A `DrmSurface` carries the native handles a KMS display has: the device descriptor as
//! `DrmDisplayHandle`, and the primary plane as `DrmWindowHandle`. They are the only route to this
//! backend's native state, so a DRM-aware renderer written outside this workspace reaches it
//! through the platform contract and needs no fork of the backend.
//!
//! No graphics API in this workspace's dependency set reads those two variants yet. wgpu answers a
//! DRM handle with "not a Vulkan-compatible handle", which is a true report of where the gap is.
//! `App::run_drm` replaces the renderer factory with one that draws through this backend.

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
pub mod app;
#[cfg(target_os = "linux")]
pub mod clipboard;
#[cfg(target_os = "linux")]
pub mod clock;
#[cfg(target_os = "linux")]
pub mod cx;
#[cfg(target_os = "linux")]
pub mod display;
#[cfg(target_os = "linux")]
pub mod input;
#[cfg(target_os = "linux")]
pub mod output;
// How the loop waits. Private because nothing outside this crate parks this loop, and the model it
// follows is stated in full in `zgui-platform-winit`'s own `park` module.
#[cfg(target_os = "linux")]
mod park;
#[cfg(target_os = "linux")]
pub mod scanout;
#[cfg(target_os = "linux")]
pub mod surface;
#[cfg(target_os = "linux")]
pub mod waker;

#[cfg(target_os = "linux")]
pub use crate::app::run;
#[cfg(target_os = "linux")]
pub use crate::clipboard::ConsoleClipboard;
#[cfg(target_os = "linux")]
pub use crate::clock::SystemClock;
#[cfg(target_os = "linux")]
pub use crate::cx::DrmCx;
#[cfg(target_os = "linux")]
pub use crate::display::{Displays, DrmDisplay};
#[cfg(target_os = "linux")]
pub use crate::output::Output;
#[cfg(target_os = "linux")]
pub use crate::scanout::{FORMAT, Scanout};
#[cfg(target_os = "linux")]
pub use crate::surface::DrmSurface;
#[cfg(target_os = "linux")]
pub use crate::waker::EventfdWaker;
