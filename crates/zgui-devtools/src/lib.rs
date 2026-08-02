//! The inspector: what a frame did, docked beside the window that did it.
//!
//! Pick an element and see what the cascade computed for it, what box the layout gave it and what
//! the frame that painted it cost. It is the tool a regression is diagnosed with, and it is also
//! the first thing somebody reaches for when their own layout does something they did not expect —
//! which is why it answers *why* rather than only *what*: the box model panel says where a width
//! came from, the frame panel says what a frame decided to redo, and the timeline says which stage
//! the time went into.
//!
//! # Wiring it in
//!
//! Two lines, because the inspector needs two things the framework keeps apart. A **view** to draw
//! itself in, which goes around the application's root, and a **probe** to read the frame through,
//! because a view can see the document but not the frame that painted it.
//!
//! ```no_run
//! use zgui::prelude::*;
//! use zgui_devtools::{DevTools, Inspector, InspectorProps};
//!
//! # #[component]
//! # fn Body() -> impl IntoView { view! { column() } }
//! fn main() -> Result<(), zgui::Error> {
//!     let tools = DevTools::new();
//!     app()
//!         .with_stylesheet(zgui_devtools::SHEET)
//!         .with_probe(tools.probe())
//!         .run(move || {
//!             let tools = tools.clone();
//!             view! { Inspector(tools = tools) {Body()} }
//!         })
//! }
//! ```
//!
//! Nothing in the framework depends on this crate, so an application that does not name it carries
//! none of it: no panel, no probe, no per-frame sampling. That is the whole of what makes the
//! inspector optional, and it is why the wiring is explicit rather than a flag on the application.
//!
//! # How it is opened
//!
//! <kbd>F12</kbd> opens and closes the panel. <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> starts
//! *picking*: the next element the pointer moves over is outlined and shown, and clicking freezes
//! the choice. <kbd>Escape</kbd> leaves picking without choosing. <kbd>F8</kbd> freezes the panel
//! on the frame it is showing, which is how a value that only exists for one frame is read.
//!
//! The chord works on a window nobody has touched yet, which is not free: a key is delivered along
//! the path to whatever holds focus, and a window in which nothing does routes one to the document
//! root and no further. So the panel registers itself as a window shortcut rather than merely
//! listening for a key, and hears one wherever focus is and whether or not there is any.
//!
//! # What it costs while it is open
//!
//! Nothing, on a document that is not moving. What the panel shows is compared against what it is
//! already showing before anything is published, so a frame that repeated the last one leaves it
//! untouched and asks for nothing further — and the two samples that cannot help but differ every
//! frame, because they include the panel's own re-render, are published on a cadence rather than
//! from every frame. A window with the panel open therefore idles exactly as one with it shut does
//! until something happens.
//!
//! <kbd>F8</kbd> is the stronger setting: frozen, the probe returns immediately, so the panel holds
//! the frame it is showing however much the window goes on to do.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod panel;
mod probe;
mod sample;
mod state;

pub use crate::panel::{Inspector, InspectorProps, SHEET};
pub use crate::state::{DevTools, Tab};
