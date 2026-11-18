//! The browser backend: one canvas in a page, driven by the browser's callbacks.
//!
//! Layer L2: backends.
//!
//! A browser gives a program no loop to block in. Each callback of the page (an input event, an
//! animation frame, a timer) runs one turn of the application and returns. `run` installs the
//! application and returns at once; the page keeps it alive.
//!
//! | Module | Content |
//! |---|---|
//! | `driver` | the turn: events in, wakes, the idle policy, the next callback |
//! | `surface` | the canvas as a `zgui_platform::Surface` |
//! | `input` | pointer, keyboard and wheel events in the framework's vocabulary |
//! | `aria` | an accessibility node in the vocabulary of a page |
//! | `clipboard` | the page's asynchronous clipboard |
//! | `clock` | the page's monotonic clock |
//! | `mount` | where the canvas comes from |
//! | `fetch` | bytes from a URL, and the fonts a page declares |
//! | `image` | images, decoded by the browser |
//! | `task` | local futures and deferred work |
//!
//! On a target other than `wasm32` the crate holds only the translation that needs no browser:
//! how a wheel event is classified, which key a name means, and what an input event asks for.
//! That part is tested on the machine that builds it.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
mod a11y;
pub mod aria;
#[cfg(target_arch = "wasm32")]
pub mod clipboard;
#[cfg(target_arch = "wasm32")]
pub mod clock;
#[cfg(target_arch = "wasm32")]
mod console;
#[cfg(target_arch = "wasm32")]
mod cx;
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "only the browser build calls what its tests check"
    )
)]
mod density;
#[cfg(target_arch = "wasm32")]
pub mod driver;
#[cfg(target_arch = "wasm32")]
pub mod fetch;
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "only the browser build calls what its tests check"
    )
)]
pub mod image;
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
pub use crate::driver::{
    canvas_of, enable_accessibility, is_running, on_context_change, resume, run, stop, suspend,
};
#[cfg(target_arch = "wasm32")]
pub use crate::fetch::{fetch, page_fonts};
#[cfg(target_arch = "wasm32")]
pub use crate::image::{image_pixels, image_size};
#[cfg(target_arch = "wasm32")]
pub use crate::mount::{Mount, canvas, query, subpixel_text_suits};
#[cfg(target_arch = "wasm32")]
pub use crate::task::{defer, note, report, report_panics, spawn_local};
/// The canvas element type, for a crate that draws into one without naming the browser's
/// interfaces itself.
#[cfg(target_arch = "wasm32")]
pub use web_sys::HtmlCanvasElement;
