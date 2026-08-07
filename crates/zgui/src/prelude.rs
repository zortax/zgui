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
