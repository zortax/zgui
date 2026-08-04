//! The view layer: what a user interface is described as, and the three seams it is described
//! against.
//!
//! A view is a value. Building one creates nodes in a backend's tree and hands back the state
//! needed to update them in place; updating one never re-creates a node that did not change. All
//! of that is written against three traits — [`Dom`], [`ViewHost`] and [`EventSink`] — and against
//! nothing else, so the same view code drives a native document, a browser's own nodes or a test
//! backend without changing a line.
//!
//! # The three seams
//!
//! | Trait | What it answers |
//! |---|---|
//! | [`Dom`] | the node tree: create, insert, detach, set an attribute, register a listener, observe geometry |
//! | [`ViewHost`] | the engine: where a box ended up, what is focused, what is selected, what to run half a second from now |
//! | [`EventSink`] | commands a handler issues, carried out after the dispatch that issued them |
//!
//! Each is object-safe and reached through a cheap cloneable handle — [`DomHandle`] and
//! [`HostHandle`] — threaded through [`BuildCx`]. There is no thread-global and no `&'static`
//! sleight of hand, which is what makes two windows in one process correct by construction: each
//! builds through its own context, and every [`NodeId`] carries the [`DocumentId`] it was minted
//! for.
//!
//! # Writing a view
//!
//! ```
//! use std::rc::Rc;
//! use zgui_interned::ElementName;
//! use zgui_reactive::prelude::*;
//! use zgui_reactive::{Mounted, RwSignal, flush, install};
//! use zgui_view::stub::{StubDom, StubHost};
//! use zgui_view::{Anchor, BuildCxOwned, DocumentId, DomHandle, HostHandle, IntoView, View};
//!
//! install().unwrap();
//! let backend = Rc::new(StubDom::new(DocumentId::FIRST));
//! let dom = DomHandle::from_rc(backend.clone());
//! let window = Mounted::new();
//! let cx = BuildCxOwned::new(
//!     dom.clone(),
//!     HostHandle::new(StubHost::default()),
//!     window.owner().clone(),
//!     DocumentId::FIRST,
//! );
//!
//! let count = window.with(|| RwSignal::new(0));
//! let root = dom.create_element(ElementName::new("box"));
//!
//! // A closure is a reactive hole: one effect, and the node is written only when the value
//! // actually changes.
//! let mut state =
//!     window.with(|| (move || count.get().to_string()).into_view().build(&mut cx.cx()));
//! state.mount(&dom, root, None);
//! assert_eq!(backend.text_content(root), "0");
//!
//! count.set(41);
//! flush();
//! assert_eq!(backend.text_content(root), "41");
//!
//! state.unmount(&dom);
//! window.unmount();
//! ```
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`dom`] | the node-tree seam, its handle, and geometry observation |
//! | [`host`] | the engine seam, focus traps, scroll targets and scheduled callbacks |
//! | [`cx`] | [`BuildCx`], [`BuildCxOwned`] and the window's host context |
//! | [`view`] | [`View`], [`Anchor`], [`IntoView`], [`AnyView`] and the built-in conversions |
//! | [`value`] | reactive attribute values, and what may be written as one |
//! | [`binding`] | attribute, class, style, state and accessibility bindings |
//! | [`event`] | listener registration, the typed context and the command sink |
//! | [`flow`] | [`Show`], [`For`], [`Suspense`], [`Transition`], [`ErrorBoundary`], [`Portal`], [`Dynamic`] |
//! | [`node_ref`] | [`NodeRef`], the observation signals and the imperative escape hatches |
//! | [`sheet`] | style sheets a view installs for itself |
//! | [`time`] | [`set_timeout`] and [`set_interval`] |
//! | [`expansion`] | the root the `view!` expansion names its crates through |
//! | `stub` | an in-memory backend for examples and tests, behind the `stub-backend` feature |

#![deny(missing_docs)]
#![forbid(unsafe_code)]

#[cfg(test)]
mod fixture;

pub mod binding;
pub mod cx;
pub mod dom;
pub mod event;
pub mod expansion;
pub mod flow;
pub mod host;
pub mod id;
#[cfg(feature = "instrument")]
pub mod instrument;
pub mod node_ref;
pub mod prelude;
pub mod scroll;
pub mod sheet;
#[cfg(any(test, feature = "stub-backend"))]
pub mod stub;
pub mod time;
pub mod value;
pub mod view;

pub use crate::binding::{A11yBinding, AttrEntry, Attrs, Binding, Classes};
pub use crate::cx::{BuildCx, BuildCxOwned, current_host, current_observations, provide_host};
pub use crate::dom::{
    Dom, DomHandle, ListenerId, ObservationHandle, ObservationSink, Observed, ObservedValue,
    OverlayLayer,
};
pub use crate::event::{
    AnyEvent, DiscardCommands, EventControl, EventCx, EventSink, EventType, EventView,
    ListenerRegistration, erase, events, handler,
};
pub use crate::flow::{
    Await, Dynamic, ErrorBoundary, ErrorBoundaryState, For, ForProps, ForPropsBuilder, Portal,
    PortalProps, PortalPropsBuilder, Show, ShowProps, ShowPropsBuilder, ShowState, Suspense,
    SuspenseContext, Transition, ViewError, report_error,
};
pub use crate::host::{
    FocusMove, FocusTrap, FocusTrapId, FocusTrapOptions, HostHandle, Repeat, TimerId, ViewHost,
    WindowShortcut,
};
pub use crate::id::{DOCUMENT_COUNT, DocumentId, NodeId};
pub use crate::node_ref::{ListenerGuard, NodeRef, ObservationRegistry, focused_node};
pub use crate::scroll::{ScrollBehavior, ScrollPosition, ScrollTarget};
pub use crate::sheet::{Stylesheet, install_stylesheet, remove_stylesheet};
pub use crate::time::{IntervalHandle, TimeoutHandle, Timers, set_interval, set_timeout};
pub use crate::value::{IntoReactiveValue, ReactiveValue};
pub use crate::view::{
    Anchor, AnyView, Children, ChildrenFn, ComponentMeta, Either, IntoView, Scoped, View,
};

/// The interned names an attribute is written with.
///
/// They are re-exported because they appear in this crate's own signatures — a class list is a
/// list of [`ClassName`], an attribute is written under an [`AttrName`] — so a consumer that
/// writes one never needs to name the crate they are defined in.
pub use zgui_interned::{AttrName, ClassName, CustomPropertyName, ElementName, Ident};

/// The vocabulary the view layer and the document agree on by value.
///
/// Re-exported for the same reason as the names above: every one of these appears in a signature
/// on [`Dom`], [`Attrs`] or [`A11yBinding`].
pub use zgui_vocab::{EventKind, ListenerOptions, PropKey, PropValue, Role, Semantics, UiState};
