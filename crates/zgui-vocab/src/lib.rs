//! The words the view layer and the document both have to say.
//!
//! Two halves of this framework describe the same elements and never speak to each other. A view
//! says what an element *is* — a button, checked, labelled by that text over there, listening for
//! a click. A document stores that, matches selectors against it and projects it to whatever is
//! asking. Between them sits one trait, and every signature in that trait names a type: not the
//! view's types, and not the document's, because either choice would make one depend on the other
//! and the framework would stop being replaceable at exactly the seam it exists to keep open.
//!
//! Those types live here. That is the whole of it — no behaviour, no engine, no platform, and a
//! dependency list of three crates.
//!
//! | Area | Types |
//! |---|---|
//! | Meaning | [`Semantics`], [`A11y`], [`Role`], [`Relations`], [`SemanticFlags`] |
//! | Interaction state | [`UiState`] |
//! | Imperative properties | [`PropKey`], [`PropValue`] |
//! | Events | [`EventKind`], [`Payload`] and its per-event structs, [`ListenerOptions`] |
//! | Dispatch control | [`Phase`], [`Propagation`], [`DefaultAction`], [`route()`] |
//! | Shared scalars | [`Modifiers`], [`Timestamp`], [`SharedString`] |
//!
//! ```
//! use zgui_vocab::{A11y, EventKind, ListenerOptions, Role, Semantics, UiState};
//!
//! // What an element is.
//! let semantics: Semantics = A11y::new(Role::Button).label("Save").into();
//! assert_eq!(semantics.role, Role::Button);
//!
//! // What state it is in, written so that contradictory states cannot coexist.
//! let state = UiState::EMPTY.apply(UiState::DISABLED, true);
//! assert!(!state.contains(UiState::ENABLED));
//!
//! // What it listens for, and how.
//! let (kind, options) = (EventKind::Click, ListenerOptions::DEFAULT);
//! assert_eq!(kind.web_name(), "click");
//! assert!(!options.capture);
//! ```
//!
//! # What is deliberately absent
//!
//! There is nothing reactive here. Every value is already resolved, so the same type can be
//! written by a view that computes it from signals, stored by a document that has never heard of
//! signals, and read by a projection that runs somewhere else entirely. A layer that wants to
//! describe an element in terms of values that change over time builds its own type and lowers it
//! to one of these at the moment of writing.
//!
//! There is nothing platform-shaped here either. Events are described by what they mean, not by
//! how a windowing system delivered them.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod a11y;
pub mod event;
pub mod modifiers;
pub mod prop;
pub mod state;
pub mod text;
pub mod time;

#[cfg(test)]
mod api;

pub use crate::a11y::semantics::{Numeric, Relations, SemanticFlags, SetPosition, TablePosition};
pub use crate::a11y::{
    A11y, AriaCurrent, AutoComplete, HasPopup, Invalid, Live, NodeId, Orientation, Role, Semantics,
    SortDirection, TextDirection, Toggled,
};
pub use crate::event::payload::{
    AnimationEvent, AnimationPhase, DropEvent, FocusCause, FocusEvent, ImeEvent, Key, KeyCode,
    KeyEvent, KeyLocation, KeyState, NamedKey, PhysicalKey, PointerAction, PointerButton,
    PointerEvent, PointerId, PointerKind, PointerSample, Pseudo, ScrollDelta, ScrollEvent,
    ScrollPhase, TextEvent, TransitionEvent, TransitionPhase, UnknownKeyCode, UnknownNamedKey,
    ValueChange, ValueEvent, WheelEvent,
};
pub use crate::event::{
    DefaultAction, EventKind, ListenerOptions, Listeners, Path, Payload, PayloadKind, Phase,
    Propagation, RouteStep, UnknownEventKind, route,
};
pub use crate::modifiers::Modifiers;
pub use crate::prop::{PropKey, PropValue};
pub use crate::state::UiState;
pub use crate::text::SharedString;
pub use crate::time::Timestamp;
