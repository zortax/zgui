//! The compositor, spoken to directly.
//!
//! This is the platform contract implemented on Wayland with no windowing library in between. It
//! exists beside [`zgui-platform-winit`], which keeps macOS, Windows and X11, because four things
//! this desktop needs cannot be reached through that seam:
//!
//! * **Frames paced against the compositor.** A Wayland client is told when to draw, by a
//!   `wl_surface.frame` callback. A portable backend that turns that callback into a lock on
//!   redraw delivery serialises the present against the next frame's processor work and quantises
//!   the frame period to whole refresh intervals. Here the callback is the display's clock and
//!   never a lock: a surface with nothing owed answers a redraw in the same turn it was asked.
//! * **Presentation that does not block.** The graphics API's own window-system integration
//!   implements first-in-first-out presentation by waiting on those same callbacks. A surface the
//!   compositor stops drawing receives none, so the acquisition blocks the thread that also reads
//!   input until the driver's timeout — a freeze rather than a dropped frame. This backend paces
//!   frames itself and says so, and the swap chain is configured never to wait.
//! * **Real visibility.** A compositor says when it has stopped repainting a surface. Without that
//!   an animation behind a hidden window runs the whole pipeline for ever.
//! * **The shell protocols.** A pop-up placed by the compositor, and a surface that is a panel, a
//!   wallpaper or a lock screen rather than a window.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`driver`] | the loop, the state every protocol handler is written on, and [`run`] |
//! | [`conn`] | the connection and every global bound on it |
//! | [`surface`] | one surface: its role, its configure sequence, its scale and its chrome |
//! | [`frame`] | when to draw: the callbacks, the timing, the visibility and the watchdog |
//! | [`output`] | what is known about the outputs |
//! | [`cx`] | the borrowed context handed to every callback |
//! | [`clock`] | the monotonic clock, and the compositor's placed on it |
//! | [`waker`] | how another thread reaches a parked loop |
//! | [`theme`] | the desktop's light or dark preference |
//! | [`capabilities`] | what this desktop turned out to be able to do |
//!
//! [`zgui-platform-winit`]: https://docs.rs/zgui-platform-winit

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for exactly one reason: a graphics API is handed
// the compositor's display and surface pointers, and the borrow that wraps them as window handles
// is unsafe because nothing in the type system ties them to the objects they came from. Every
// unsafe block states what makes it sound.
#![allow(unsafe_code)]

pub mod capabilities;
pub mod clipboard;
pub mod clock;
pub mod conn;
pub mod cx;
pub mod driver;
pub mod frame;
pub mod input;
pub mod output;
pub mod surface;
pub mod theme;
pub mod waker;

pub use crate::clipboard::WaylandClipboard;
pub use crate::clock::{Monotonic, SystemClock};
pub use crate::cx::WaylandCx;
pub use crate::driver::{WaylandApp, WaylandState, run};
pub use crate::frame::{Pacer, Presentation, Timing, Visibility, Watchdog};
pub use crate::input::desktop_scroll_settings;
pub use crate::surface::WaylandSurface;
pub use crate::waker::PingWaker;
