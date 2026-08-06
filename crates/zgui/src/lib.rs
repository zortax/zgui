//! A user interface framework for native applications: components, signals, CSS and a real
//! window, with no browser and no JavaScript anywhere.
//!
//! An interface is written as component functions returning views. State is signals. Layout and
//! appearance are CSS — the cascade, the selectors, flexbox, grid, gradients, filters and
//! transforms, over a renderer of this framework's own. A window is a real window on the desktop,
//! and a listener is a native callback.
//!
//! This crate is the one an application depends on. Everything below it — the document, the style
//! engine, layout, text, painting, the renderer, the platform — is reachable through here, and an
//! application that only wants to describe an interface never needs to name any of it.
//!
//! # A whole application
//!
//! ```no_run
//! use zgui::prelude::*;
//!
//! /// A number, and two buttons that change it.
//! #[component]
//! fn Counter(
//!     /// Where the count starts.
//!     #[prop(default = 0)]
//!     start: i32,
//! ) -> impl IntoView {
//!     let (count, set_count) = signal(start);
//!
//!     view! {
//!         column(class = "counter", a11y:role = Role::Group, a11y:label = "Counter") {
//!             text(class = "counter__value") {{move || count.get().to_string()}}
//!             row(class = "counter__buttons") {
//!                 control(class = "button", on:click = move |_| set_count.update(|n| *n -= 1)) {
//!                     "-"
//!                 }
//!                 control(class = "button", on:click = move |_| set_count.update(|n| *n += 1)) {
//!                     "+"
//!                 }
//!             }
//!         }
//!     }
//! }
//!
//! const SHEET: &str = css!(
//!     ":root { background: #14161a; color: #f2f4f8; font-family: sans-serif }
//!      .counter { gap: 16px; padding: 24px; align-items: center }
//!      .counter__value { font-size: 48px; font-weight: 700 }
//!      .button { padding: 8px 20px; border-radius: 8px; background: #2b6cff }"
//! );
//!
//! fn main() -> Result<(), zgui::Error> {
//!     app()
//!         .with_title("Counter")
//!         .with_size(360.0, 240.0)
//!         .with_stylesheet(SHEET)
//!         .run(|| view! { Counter(start = 0) })
//! }
//! ```
//!
//! Four things in that are the whole model.
//!
//! **A signal is the state.** `signal(0)` hands back a reader and a writer. Nothing subscribes to
//! it but the closures that read it, and writing it re-runs exactly those.
//!
//! **A closure is a reactive hole.** `{move || count.get().to_string()}` is written once and
//! updated for ever; `{count.get().to_string()}` — the same expression without the closure — is
//! written once and never again. The difference is the type, not a keyword, and it is the whole
//! static-versus-dynamic story.
//!
//! **`on:click` is a listener**, taking part in capture and bubble, with its payload type inferred
//! from the event name. A component's own callback prop is written `on_change=…` instead, because
//! it is an ordinary prop and not a listener.
//!
//! **The appearance is CSS**, in an ordinary style sheet, checked at compile time by
//! [`css!`](macro@css). Element names have their layout before a single rule is written: a
//! [`row`](elements::row) is a horizontal flex container, a [`column`](elements::column) a
//! vertical one, [`text`](elements::text) is inline.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`prelude`] | everything an interface is written with, in one import |
//! | [`mod@app`] | [`App`], the faces it draws with, and the platform it runs on |
//! | [`elements`] | the element vocabulary, as builders, for building one by hand |
//! | [`reactive`] | signals, memos, stores, contexts, ownership and the flush |
//! | [`task`] | spawning on the UI thread, working off it, and getting back on to it |
//! | [`view`](mod@view) | the view layer's own seams: the node tree, the engine, the event sink |
//! | [`runtime`] | the frame pipeline, its windows and its timers |
//! | [`platform`] | the windowing contract, for a backend of one's own |
//! | [`render`], [`scene`], [`atlas`], [`bits`] | what a frame is drawn through, for a device of one's own |
//! | [`geom`] | the pixel spaces and the geometry every stage agrees in |
//! | [`text`] | the text seams: faces, shaping, metrics and glyph rasterisation |
//! | [`vocab`] | events, keys, roles and states, as every layer says them |
//! | [`expansion`] | the root a [`view!`](macro@view) expansion names its crates through |
//!
//! # Writing a component
//!
//! [`component`](macro@component) turns a function into a component: every argument is a named
//! prop, a prop with no attribute is required, and leaving one out is a compile error that names
//! it. A component returns `impl IntoView`, which every view — a string, a number, a tuple, an
//! `Option`, a closure, an element — already satisfies.
//!
//! ```
//! use zgui::prelude::*;
//!
//! /// A label and a value beside it.
//! #[component]
//! fn Stat(
//!     /// What the value is called.
//!     #[prop(into)]
//!     label: String,
//!     /// The value, which follows whatever produced it.
//!     value: Signal<i32>,
//! ) -> impl IntoView {
//!     view! {
//!         row(class = "stat") {
//!             label {{label}}
//!             text {{move || value.get().to_string()}}
//!         }
//!     }
//! }
//! ```
//!
//! # Running somewhere else
//!
//! [`App::run`] opens a window on the desktop this program is running on. An application that
//! needs a different platform, a different graphics device or faces of its own replaces exactly
//! that one decision — see [`App::run_on`], [`App::with_renderer`] and [`App::with_fonts`] — and
//! nothing above it changes.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod app;
pub mod error;
pub mod prelude;

pub use crate::app::{App, app};
pub use crate::error::Error;

/// A view written as nested calls, a component written as a function, and the styling macros.
pub use zgui_view_macro::{component, css, slot, style, variants, view};

/// The crates a [`view!`](macro@view) expansion names.
///
/// Nothing in it is written by hand. It is public because the code the macros generate says
/// `::zgui::expansion::…`, which is what lets a crate that writes views depend on this one alone.
pub use zgui_elements::expansion;

/// Where a glyph tile or a decoded image is kept, and how it reaches a device.
pub use zgui_atlas as atlas;
/// What a `canvas` element is drawn with: scenes, shapes and brushes.
pub use zgui_canvas as canvas;
/// The damage a frame has to redraw.
pub use zgui_bits as bits;
/// The element vocabulary, as builders.
pub use zgui_elements as elements;
/// The pixel spaces and the geometry every stage agrees in.
pub use zgui_geom as geom;
/// The windowing contract every platform backend implements.
pub use zgui_platform as platform;
/// Signals, memos, stores, contexts, ownership and the flush the frame loop drives.
pub use zgui_reactive as reactive;
/// Doing something that takes time: on the UI thread, off it, and back on to it.
pub use zgui_reactive::task;
/// What a frame is drawn through, for an application supplying a device of its own.
pub use zgui_render as render;
/// The frame pipeline, its windows and its timers.
pub use zgui_runtime as runtime;
/// The display list a frame is composed into.
pub use zgui_scene as scene;
/// Faces, shaping, metrics and glyph rasterisation, as seams.
pub use zgui_text as text;
/// A tokio runtime, as the executor behind [`task::background`].
///
/// Compiled only when the `tokio` feature is on, so an application that does not ask for a runtime
/// does not link one. See the crate's own documentation for what installing it buys.
#[cfg(feature = "tokio")]
pub use zgui_tokio as tokio;
/// The view layer: what a view is, and the three seams it is described against.
pub use zgui_view as view;
/// What every layer says the same things in: events, keys, roles, states and shared strings.
pub use zgui_vocab as vocab;
