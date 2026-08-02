//! The accessibility tree: what a document means, published to whatever is reading it aloud, and
//! what that reader asks for, routed back in.
//!
//! An application built on this framework is usable by somebody who cannot see it, and that is not
//! a feature a component opts into. Everything an assistive technology needs is already in the
//! document — the role an element declared, the label it was given, where layout put it, which
//! listener it registered — and this crate is the projection of those into the one vocabulary the
//! desktop's accessibility bus speaks.
//!
//! # The three things this does
//!
//! **It builds the tree.** [`A11yBuilder`] turns a frame into an `accesskit::TreeUpdate`. An
//! accesskit node is replace-not-patch, so an update is produced by projecting whole nodes and
//! comparing them with the ones last sent — never by hand-writing a partial node. Which nodes are
//! projected comes from the document's own accessibility marks, so a frame that changed one label
//! sends one node.
//!
//! **It emits relations.** `labelled_by`, `described_by`, `controls`, `owns`, `radio_group`,
//! `active_descendant`, `popup_for` and `error_message` are what make a combobox, a dialog, a menu
//! and a labelled field describable at all: in each case the tree structure says the wrong thing
//! and the relation says the right one. Every identifier is checked before it is written, because
//! a consumer resolves one without checking it — see [`integrity`].
//!
//! **It routes actions back.** An inbound `Action::Click` becomes [`Intent::Dispatch`] of an
//! ordinary click on the target, which the frame then dispatches down the same path a pointer
//! takes. That is the whole activation story, and it is why no component contains separate
//! accessibility activation logic.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`world`] | everything a projection reads, in one borrow |
//! | [`project`] | one document node, turned into one accessibility node |
//! | [`build`] | which nodes to project, and the update they become |
//! | [`action`] | an inbound action, in the document's own terms |
//! | [`integrity`] | the invariant that keeps a consumer from panicking |
//! | [`mod@dump`] | an update as text, so a relation is visible in a review |
//!
//! ```
//! use zgui_a11y::{A11yBuilder, World};
//! use zgui_dom::{Document, EverythingMatters};
//! use zgui_interned::ElementName;
//! use zgui_layout::tree::store::LayoutStore;
//! use zgui_vocab::{A11y, Role};
//!
//! let document = Document::new();
//! document
//!     .edit(&EverythingMatters, |edit| {
//!         let root = edit.create_element(ElementName::new("root"));
//!         edit.insert_before(document.document_index(), root, None);
//!         let button = edit.create_element(ElementName::new("control"));
//!         edit.insert_before(root, button, None);
//!         edit.set_semantics(button, Some(A11y::new(Role::Button).label("Save").into()));
//!     })
//!     .expect("a fresh document is not poisoned");
//!
//! let layout = LayoutStore::new(document.store().document());
//! let placements = zgui_scene::Placements::EMPTY;
//! let world = World {
//!     document: &document,
//!     layout: &layout,
//!     placements: &placements,
//!     scale: 1.0,
//!     focus: None,
//! };
//!
//! let mut builder = A11yBuilder::new();
//! let update = builder.build(&world);
//! assert!(zgui_a11y::dump(&update).contains("Button label=\"Save\""));
//! assert!(zgui_a11y::dangling(&update, builder.retained()).is_empty());
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod action;
pub mod build;
pub mod dump;
pub mod id;
pub mod integrity;
pub mod project;
pub mod world;

pub use crate::action::{
    Action, ActionData, ActionRequest, Intent, Point, Scroll, Step, intent_of,
};
pub use crate::build::{A11yBuilder, A11yTree, TreeId, TreeUpdate};
pub use crate::dump::dump;
pub use crate::id::{NodeId, to_a11y, to_document};
pub use crate::integrity::{Dangling, Mention, dangling};
pub use crate::world::World;
