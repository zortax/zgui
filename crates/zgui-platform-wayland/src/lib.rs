//! The compositor, spoken to directly.
//!
//! This is the platform contract implemented on Wayland with no windowing library in between. It
//! exists beside [`zgui-platform-winit`], which keeps macOS, Windows and X11, because four things
//! this desktop needs cannot be reached through that seam:
//!
//! * **Frame preparation overlaps presentation.** Redraw delivery never waits for a compositor
//!   callback. The default renderer uses a bounded FIFO presentation worker, allowing the UI to
//!   prepare the next scene while the previous buffer waits for display.
//! * **Presentation waits outside the UI.** A dedicated callback queue checks compositor readiness
//!   before entering graphics-API locks. Hidden surfaces cannot stall input behind a driver wait.
//!   Buffer scale, viewport and geometry travel with their matching buffer, including on resize.
//! * **Real visibility.** A compositor says when it has stopped repainting a surface. Without that
//!   an animation behind a hidden window runs the whole pipeline for ever.
//! * **The shell protocols.** A pop-up placed by the compositor, and a surface that is a panel, a
//!   wallpaper or a lock screen rather than a window.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | `driver` | the loop, the state every protocol handler is written on, and `run` |
//! | `conn` | the connection and every global bound on it |
//! | `surface` | one surface: its role, its configure sequence, its scale and its chrome |
//! | `frame` | when to draw: the callbacks, the timing, the visibility and the watchdog |
//! | `output` | what is known about the outputs |
//! | `cx` | the borrowed context handed to every callback |
//! | `clock` | the monotonic clock, and the compositor's placed on it |
//! | `waker` | how another thread reaches a parked loop |
//! | `theme` | the desktop's light or dark preference |
//! | `capabilities` | what this desktop turned out to be able to do |
//!
//! [`zgui-platform-winit`]: https://docs.rs/zgui-platform-winit

// Every module is gated on the platforms with a Wayland compositor, so the names above are in
// backticks rather than intra-doc links. A link to a gated item is broken on every other platform,
// and this workspace denies a broken one.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for exactly one reason: a graphics API is handed
// the compositor's display and surface pointers, and the borrow that wraps them as window handles
// is unsafe because nothing in the type system ties them to the objects they came from. Every
// unsafe block states what makes it sound.
#![allow(unsafe_code)]

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod capabilities;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod clipboard;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod clock;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod conn;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod cx;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod driver;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod frame;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod input;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod output;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod surface;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod theme;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub mod waker;

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub use crate::clipboard::WaylandClipboard;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub use crate::clock::{Monotonic, SystemClock};
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub use crate::cx::WaylandCx;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub use crate::driver::{WaylandApp, WaylandState, run};
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub use crate::frame::{Pacer, Presentation, Timing, Visibility, Watchdog};
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub use crate::input::desktop_scroll_settings;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub use crate::surface::WaylandSurface;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub use crate::waker::PingWaker;
