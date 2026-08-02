//! What an event means for a document: where it landed, what state it changed, and which
//! listeners it reaches.
//!
//! Everything a person does arrives here as a platform event and leaves as three answers:
//!
//! * **where it landed** — the element under the pointer, or the focused element for a key, and
//!   the path of ancestors from the root down to it ([`hit`]);
//! * **what changed about the document** — `:hover`, `:active`, `:focus`, `:focus-visible` and
//!   `:focus-within`, written into the document itself so that ordinary selectors match on them
//!   and there is no second state machine anywhere ([`state`]);
//! * **which listeners it reaches, in which order** — a list of `(element, listener, phase)`
//!   ([`dispatch`]), plus the framework's own default behaviour ([`dispatch::defaults`]).
//!
//! # What this crate deliberately does not do
//!
//! **It never runs a handler.** A handler's argument is a view-layer type, and a system that could
//! name one would make the layer beneath the view layer depend on it. Resolving the order and
//! running the handlers are two jobs: this crate answers *which* listener runs *when*, by name,
//! and whatever holds the handlers looks each name up and calls it. That split is why
//! [`dispatch::resolve`] returns [`Step`]s and not results.
//!
//! **It owns no index.** Which fragments are under a point is answered by the layout engine, which
//! is where the pass that writes those entries lives. What this crate owns is the other half of
//! the question: turning fragments into the *element* path dispatch walks — which differs from the
//! box path whenever an element generates boxes that are not its own, and is why `tr:hover td`
//! works over a row whose `display: contents` makes its cells children of the table.
//!
//! # The shape of it
//!
//! | Module | What it answers |
//! |---|---|
//! | [`normalize`] | what the window said, in units the document can use |
//! | [`hit`] | which element is under a point, and its path to the root |
//! | [`capture`] | which element is receiving the pointer regardless of position |
//! | [`state`] | which interaction bits the document now carries |
//! | [`focus`] | what can be focused, in what order, and what confines it |
//! | [`gesture`] | what a sequence of raw touches means |
//! | [`drag`] | what is being dragged within the window, and where it may land |
//! | [`dispatch`] | which listeners run, in which order, and what the framework does itself |
//! | [`router`] | all of the above, driven from one event |
//!
//! ```
//! use zgui_dom::{Document, EverythingMatters};
//! use zgui_input::dispatch::{Plan, resolve};
//! use zgui_input::hit::HitChain;
//! use zgui_input::state::Interaction;
//! use zgui_interned::ElementName;
//! use zgui_vocab::{EventKind, ListenerOptions, Phase, UiState};
//!
//! // Every change goes through the document's own batch, which is what records what the style
//! // engine needs and marks the path a traversal has to descend.
//! let document = Document::new();
//! let (root, button) = document
//!     .edit(&EverythingMatters, |edit| {
//!         let root = edit.create_element(ElementName::new("root"));
//!         edit.insert_before(document.document_index(), root, None);
//!         let button = edit.create_element(ElementName::new("control"));
//!         edit.insert_before(root, button, None);
//!         edit.add_listener(root, EventKind::Click, ListenerOptions::CAPTURE);
//!         edit.add_listener(button, EventKind::Click, ListenerOptions::DEFAULT);
//!         (root, button)
//!     })
//!     .expect("not poisoned");
//!
//! // What a press on the button landed on: the button, and everything containing it.
//! let chain = HitChain::to_root(document.store(), document.store().key_of(button));
//!
//! // What that changes about the document, which is what selectors then match on.
//! let mut interaction = Interaction::default();
//! interaction.hover.move_to(&document, &EverythingMatters, &chain);
//! assert!(document.store().core(root).ui_state().contains(UiState::HOVER));
//!
//! // And who hears about it, in order: the root on the way down, the button at the bottom.
//! let mut plan = Plan::new();
//! resolve(document.store(), &chain, EventKind::Click, &mut plan);
//! let phases: Vec<Phase> = plan.steps().iter().map(|step| step.phase).collect();
//! assert_eq!(phases, vec![Phase::Capture, Phase::Target]);
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod capture;
pub mod dispatch;
pub mod drag;
pub mod focus;
pub mod gesture;
pub mod hit;
pub mod ime;
pub mod normalize;
pub mod router;
pub mod state;

pub use crate::capture::PointerCapture;
pub use crate::dispatch::{FrameworkDefault, Step};
pub use crate::drag::{Drag, DragPhase, DragSource, Drags, DropEffect, Dropped};
pub use crate::focus::{FocusDirection, FocusSource, FocusTrapId, TrapOptions};
pub use crate::gesture::{Gesture, Gestures};
pub use crate::hit::{Hit, HitChain};
pub use crate::ime::{Ime, Preedit};
pub use crate::normalize::{HeldModifiers, InputEvent, ScrollUnits};
pub use crate::router::{Routed, Router, World};
pub use crate::state::within::Moved;
