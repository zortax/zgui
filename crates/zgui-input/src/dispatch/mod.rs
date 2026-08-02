//! Which listeners an event reaches, in which order — and what the framework itself does about it.
//!
//! This module resolves and never runs. A handler's argument is a view-layer type and this crate
//! sits below the view layer, so a call from here would invert the graph: the layer that owns hit
//! testing would depend on the layer that owns components, and no other backend could ever be put
//! underneath. So the answer is a list of names — element, listener, leg — and whatever holds the
//! handlers walks it, looks each name up, and calls.
//!
//! That split is what makes `stop_propagation` the caller's business as well. The list is the
//! whole order; a caller that has been asked to stop stops walking it. Resolving the order again
//! from inside the walk would mean re-entering the document mid-dispatch, which is exactly the
//! re-entrancy the mutation protocol exists to avoid.
//!
//! ```
//! use zgui_dom::{Document, EverythingMatters};
//! use zgui_input::dispatch::{Plan, resolve};
//! use zgui_input::hit::HitChain;
//! use zgui_interned::ElementName;
//! use zgui_vocab::{EventKind, ListenerOptions, Phase};
//!
//! let document = Document::new();
//! let button = document
//!     .edit(&EverythingMatters, |edit| {
//!         let root = edit.create_element(ElementName::new("root"));
//!         edit.insert_before(document.document_index(), root, None);
//!         let button = edit.create_element(ElementName::new("control"));
//!         edit.insert_before(root, button, None);
//!         edit.add_listener(root, EventKind::Click, ListenerOptions::CAPTURE);
//!         edit.add_listener(button, EventKind::Click, ListenerOptions::DEFAULT);
//!         button
//!     })
//!     .expect("not poisoned");
//!
//! let chain = HitChain::to_root(document.store(), document.store().key_of(button));
//! let mut plan = Plan::default();
//! resolve(document.store(), &chain, EventKind::Click, &mut plan);
//!
//! let phases: Vec<Phase> = plan.steps().iter().map(|step| step.phase).collect();
//! assert_eq!(phases, vec![Phase::Capture, Phase::Target]);
//! ```

pub mod defaults;
pub mod phases;

pub use crate::dispatch::defaults::FrameworkDefault;
pub use crate::dispatch::phases::{Plan, Step, append, resolve};
