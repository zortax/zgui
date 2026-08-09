//! The frame pipeline, in a window.
//!
//! This is where every other part of the framework becomes one running program. A person moves a
//! pointer; the platform reports it; the input system decides what was under it and which
//! listeners hear about it; this crate calls them; their bodies write signals; the reactive layer
//! marks precisely the nodes that changed; the style engine restyles those and no others; layout
//! measures what moved; the paint stage emits only what the damage reaches; and the renderer puts
//! it on the screen. Then the loop parks, and burns nothing until something happens.
//!
//! # The four things that can ask for a frame
//!
//! The list is exhaustive on purpose, because a missing entry is not a crash but a window that
//! quietly stops responding to one whole class of event.
//!
//! 1. **A change to the document.** Every mutation asks for the frame that will show it.
//! 2. **Input.** Dispatching an event asks for the frame its handlers' writes will appear in.
//! 3. **Work finishing somewhere else.** A future resolving, an image decoding, a value arriving
//!    from a worker thread — see [`wake`].
//! 4. **A deadline arriving.** A timer coming due, or an animation's next tick. A loop parked with
//!    a deadline is woken by the platform *and told the deadline was reached*; nothing draws until
//!    something turns that into a request. See [`App`].
//!
//! Requests raised from *inside* a frame are folded into one request made by the frame's last
//! phase, so a frame in which four different things each wanted another frame costs one more
//! frame, not four.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`app`] | [`App`], the platform handler, and the parking |
//! | [`window`] | one window: its document, its engines, its renderer, and its frame |
//! | [`timer`] | scheduled callbacks and the deadlines they give the loop |
//! | [`wake`] | the wake edge and the in-frame gate |
//! | [`dispatch`] | calling the listeners the input system resolved |
//! | [`host`] | the engine seam a view asks its geometry and its timers through |
//! | [`text`] | the text engine seam, and the brush table a theme change rewrites |
//! | [`editing`] | the editors a text field's model lives in, keyed by node |
//! | [`selection`] | the selection each editable node carries, and the one that is focused |
//! | [`binding`] | the seam a downstream script engine attaches to |
//!
//! ```no_run
//! use zgui_runtime::App;
//! use zgui_view::{Anchor, BuildCx, IntoView, View};
//!
//! // A window, a stylesheet, and a view built into it. `run` hands the application to a platform
//! // backend and returns when the loop finishes.
//! # fn drive(_: Box<dyn zgui_platform::AppHandler>) -> Result<(), zgui_platform::PlatformError> {
//! #     Ok(())
//! # }
//! App::new()
//!     .with_title("counter")
//!     .with_stylesheet("root { display: block }")
//!     .run(
//!         |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
//!             Box::new(zgui_elements::column().into_view().build(cx))
//!         },
//!         drive,
//!     )
//!     .expect("the application ran");
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod app;
pub mod binding;
pub mod budget;
pub mod caret;
pub mod clipboard;
pub mod commands;
pub mod dispatch;
pub mod editing;
pub mod embed;
pub mod error;
pub mod host;
mod images;
mod order;
pub mod parity;
pub mod probe;
pub mod replaced;
pub mod selection;
pub mod text;
pub mod timer;
pub mod wake;
pub mod window;
pub mod windows;

pub use crate::app::{
    App, ExitPolicy, Failure, MetricsFactory, RasterFactory, RendererFactory, Runtime, TextFactory,
    ViewFactory,
};
pub use crate::binding::{HostBinding, NoBinding};
pub use crate::budget::{BudgetReport, Budgeted, CacheId, CacheReport};
pub use crate::clipboard::{Clipboards, try_use_clipboard, use_clipboard};
pub use crate::commands::{
    CloseCallbacks, CloseResponse, WindowCommands, WindowSpec, WindowStatus, WindowToken,
};
pub use crate::editing::Editors;
pub use crate::embed::{
    EmbedHost, EmbedMaintenanceCx, EmbedMemoryReport, EmbedSyncCx, EmbedSyncReport, NoEmbeds,
};
pub use crate::error::AppError;
pub use crate::host::RuntimeHost;
pub use crate::probe::FrameProbe;
pub use crate::replaced::{IntrinsicTable, ReplacedMux};
pub use crate::selection::Selections;
pub use crate::text::TextEngine;
pub use crate::timer::Timers;
pub use crate::wake::{FrameGate, RuntimeWaker};
pub use crate::window::anim::cadence::AnimationCadence;
pub use crate::window::present::PresentPace;
pub use crate::window::resize::ResizePace;
pub use crate::window::{Window, WindowContent};
pub use crate::windows::WindowOptions;
pub use crate::windows::{
    CloseGuard, WindowHandle, WindowId, Windows, on_close_request, try_use_window, try_use_windows,
    use_window, use_windows,
};
