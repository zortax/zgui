//! Everything an application writes an interface with.
//!
//! One import brings in the reactive primitives, the authoring macros, the element vocabulary's
//! attribute types, the events, the control flow and the application entry point:
//!
//! ```
//! use zgui::prelude::*;
//! ```
//!
//! Nothing here is exclusive to it — every name is also reachable at its own path — so an
//! application that would rather import what it uses loses nothing by not importing this.
//!
//! The application *type* is deliberately absent too, and its entry point is the function
//! [`app()`](crate::app()) instead. `App` is the obvious name for a root component, a `style!` block
//! beside that component declares a type of that name, and a locally declared item shadows a
//! glob-imported one — so a prelude exporting a type called `App` would stop resolving in exactly
//! the programs most likely to import it. `#[component]` requires an upper-camel-case name, so
//! nothing a component brings into scope can be spelled `app`.
//!
//! The element *names* are deliberately absent. `<row>` and `<text>` are written as tags inside
//! [`view!`](macro@crate::view), which resolves them itself, and importing sixteen short function
//! names into an application's own namespace would shadow more than it helps. An application
//! building elements by hand names them through [`elements`](crate::elements).

pub use crate::app::{Fonts, app};
pub use crate::error::Error;

/// The windows an application has: the one a component is running in, all of them, and how to open
/// another. Every operation on a window that has closed does nothing, as does every operation this
/// desktop cannot carry out — so none of this needs a branch per platform.
pub use zgui_runtime::windows::{
    CloseGuard, WindowHandle, WindowId, WindowOptions, Windows, on_close_request, try_use_window,
    try_use_windows, use_window, use_windows,
};

/// The desktop's clipboards: what to copy onto, and what to read back.
///
/// [`ClipboardKind::Standard`] is the one a copy and a paste use.
/// [`ClipboardKind::Primary`] is the selection Linux desktops paste with the middle button;
/// everywhere else it does nothing, so copy-on-select needs no branch per platform.
pub use zgui_platform::ClipboardKind;
pub use zgui_runtime::clipboard::{Clipboards, try_use_clipboard, use_clipboard};

/// What a window can be asked to be, what it answers, and when an application stops.
///
/// The desktop's own light-or-dark preference is deliberately absent: `ColorScheme` is already the
/// name of the theme a view asks for, and a window's is a different question with the same answer
/// type. It is [`zgui::platform::ColorScheme`](zgui_platform::ColorScheme) for the rare caller that
/// overrides one window's.
pub use zgui_platform::{
    CursorStyle, Decorations, FullscreenMode, ResizeEdge, WindowIcon, WindowLevel,
};
pub use zgui_runtime::{CloseResponse, ExitPolicy};

/// Signals, memos, stores, contexts and ownership; the view layer's traits, bindings, events,
/// control flow and geometry observation; and the accessibility vocabulary.
pub use zgui_view::prelude::*;

/// What a listener's payload is read in terms of: which key, which button, which modifiers, and
/// which of the states a view may assert.
pub use zgui_vocab::{
    EventKind, Key, KeyCode, Modifiers, NamedKey, PhysicalKey, PointerButton, ScrollDelta, UiState,
};

/// What a `prop:` binding carries. Setting one from a view needs the value type in scope, so it
/// belongs here beside the tags that take them.
pub use zgui_vocab::prop::PropValue;

/// A view written as nested tags, a component written as a function, and the styling that goes
/// with them.
pub use zgui_view_macro::{component, css, slot, style, variants, view};

/// What `tabindex` takes.
pub use zgui_elements::Focus;

/// Drawing into a `canvas` element: the retained handle, and what a draw closure receives.
pub use zgui_elements::{CanvasHandle, DrawCx};

/// Filling a `surface` element: the binding methods, the producer handle, and the on-demand
/// renderer contract.
pub use zgui_wgpu::{SurfaceElementExt, SurfaceHandle, SurfaceRenderer};

/// Implementing an element rather than composing one: the trait, its contexts, and the handle
/// that reaches a mounted one.
pub use zgui_custom::{CustomElement, CustomHandle, CustomLayoutCx, CustomMeasured, ScenePainter};

/// Drawing with a shader of one's own: declaring an effect, the handle it is drawn through, and
/// the painter method that draws it.
pub use zgui_shader::{ShaderEffect, ShaderHandle, ShaderPainterExt, ShaderParams, shader};
