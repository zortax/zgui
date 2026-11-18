//! The browser backend: one canvas in a page, driven by the browser's callbacks.
//!
//! Layer L2: backends.
//!
//! A browser gives a program no loop to block in. Each callback of the page (an input event, an
//! animation frame, a timer) runs one turn of the application and returns. [`run`] installs the
//! application and returns at once; the page keeps it alive.
//!
//! | Module | Content |
//! |---|---|
//! | [`driver`] | the turn: events in, wakes, the idle policy, the next callback |
//! | [`surface`] | the canvas as a [`zgui_platform::Surface`] |
//! | [`input`] | pointer, keyboard and wheel events in the framework's vocabulary |
//! | [`clipboard`] | the page's asynchronous clipboard |
//! | [`clock`] | the page's monotonic clock |
//! | [`mount`] | where the canvas comes from |
//! | [`fetch`] | bytes from a URL, and the fonts a page declares |
//! | [`task`] | local futures and deferred work |
//!
//! On a target other than `wasm32` the crate holds only the translation that needs no browser:
//! how a wheel event is classified, which key a name means, and what an input event asks for.
//! That part is tested on the machine that builds it.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
pub mod clipboard;
#[cfg(target_arch = "wasm32")]
pub mod clock;
#[cfg(target_arch = "wasm32")]
mod cx;
#[cfg(target_arch = "wasm32")]
pub mod driver;
#[cfg(target_arch = "wasm32")]
pub mod fetch;
pub mod input;
#[cfg(target_arch = "wasm32")]
mod listen;
#[cfg(target_arch = "wasm32")]
pub mod mount;
#[cfg(target_arch = "wasm32")]
pub mod surface;
#[cfg(target_arch = "wasm32")]
pub mod task;
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "only the browser build calls what its tests check"
    )
)]
mod text_input;
#[cfg(target_arch = "wasm32")]
mod waker;

#[cfg(target_arch = "wasm32")]
pub use crate::driver::{canvas_of, run, stop};
#[cfg(target_arch = "wasm32")]
pub use crate::fetch::{fetch, page_fonts};
#[cfg(target_arch = "wasm32")]
pub use crate::mount::{Mount, canvas, query};
#[cfg(target_arch = "wasm32")]
pub use crate::task::{defer, report, spawn_local};
/// The canvas element type, for a crate that draws into one without naming the browser's
/// interfaces itself.
#[cfg(target_arch = "wasm32")]
pub use web_sys::HtmlCanvasElement;
