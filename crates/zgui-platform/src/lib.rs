//! The contract a windowing system has to satisfy, and nothing about any particular one.
//!
//! Everything a user interface needs from the machine it is running on arrives through the traits
//! in this crate: a thing to draw into, input from a person, the time, the clipboard, the
//! accessibility channel, and a way for another thread to say that something happened. Nothing
//! above this layer names a windowing library, and this crate does not either — not in its code,
//! and not in its manifest.
//!
//! That absence is the entire point, and it is worth being concrete about what it buys. A
//! windowing library's next major version renames its size accessors, splits its lifecycle
//! callbacks, merges touch into the pointer stream, rewrites drag and drop, and removes its
//! user-event payload. Every one of those changes reaches every crate in a framework that names it
//! directly. Behind this seam they reach one.
//!
//! # Two implementations, and why there are two
//!
//! A boundary with one implementation behind it is not a boundary; it is an indirection that
//! happens to match whatever was built first. Two are planned, and they are as different as this
//! contract has to tolerate:
//!
//! * a **windowing backend** — a real event loop, real windows, real accessibility adapters, the
//!   desktop's clipboards, and surfaces a graphics API can draw into; and
//! * a **headless backend** — a clock that only moves when a test moves it, input that is scripted
//!   rather than received, a clipboard that is a value in memory, and a surface that is a buffer.
//!
//! The second is not a lesser version of the first. It is what makes a seven-hundred-millisecond
//! delay testable in a microsecond, what lets most of this framework's tests run with no display
//! server at all, and what turns the frame loop's own parking behaviour — the hardest thing here
//! to get right — into something a test can assert on.
//!
//! Several details of these traits exist *only* because both have to fit. The clock is a trait
//! rather than a call to the system. A clipboard read is a request answered later rather than a
//! value returned now. The graphics handles are a separate trait a surface may decline to
//! implement. Each of those would be simpler if there were only ever going to be one backend, and
//! each of them is what makes the second one possible.
//!
//! # The shape of it
//!
//! | Trait | What it is |
//! |---|---|
//! | [`AppHandler`] | what the backend calls, in the order it calls it |
//! | [`PlatformCx`] | what the backend offers, for the duration of one callback |
//! | [`Surface`] | a thing that can be drawn into and interacted with |
//! | [`GpuSurface`] | the same, seen by a graphics API |
//! | [`Clock`] | where the time comes from |
//! | [`Waker`] | how another thread asks the loop to wake up |
//! | [`Clipboard`] | the desktop's clipboards |
//! | [`ScrollSettings`] | what a wheel detent means on this desktop |
//!
//! and one plain value, [`PlatformCapabilities`], which is how a component asks whether something
//! is possible without ever asking which desktop it is on.
//!
//! # Input is modelled on where windowing is going, not where it has been
//!
//! A mouse, a finger and a stylus produce one kind of event here, told apart by a field. Touch is
//! not a second event stream. A drag carries its whole set of paths at every stage rather than one
//! path per event. Those two are the parts of a windowing library's own input overhaul that would
//! otherwise reach component code, so they are settled once, here, at the boundary — and a backend
//! written against an older model synthesises them rather than the other way round.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod app;
pub mod capabilities;
pub mod clipboard;
pub mod clock;
pub mod cx;
pub mod error;
pub mod monitor;
pub mod scroll;
pub mod surface;
pub mod theme;
pub mod waker;

#[cfg(test)]
mod api;
#[cfg(test)]
mod backends;

pub use crate::app::{AppHandler, IdlePolicy, WakeReason};
pub use crate::capabilities::PlatformCapabilities;
pub use crate::clipboard::{
    Clipboard, ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
    ClipboardSerials, ClipboardWriteOptions,
};
pub use crate::clock::{Clock, VirtualClock};
pub use crate::cx::PlatformCx;
pub use crate::error::{PlatformError, Unsupported};
pub use crate::monitor::{MonitorInfo, refresh_interval};
pub use crate::scroll::ScrollSettings;
pub use crate::surface::{
    BadIcon, CursorStyle, DecorationSource, Decorations, DragEvent, FullscreenMode, GpuSurface,
    ResizeEdge, Surface, SurfaceAttributes, SurfaceEvent, SurfaceId, TextInput, TextInputPurpose,
    WindowIcon, WindowLevel,
};
pub use crate::theme::ColorScheme;
pub use crate::waker::Waker;
