//! What a user interface is described as.
//!
//! Two traits carry the whole model. [`View`] says how a value becomes nodes and how it updates
//! them; [`Anchor`] says where those nodes are and how to take them away. Everything else in this
//! module is an implementation of the first for a type an author already writes — a string, a
//! number, a tuple, an `Option`, a closure — or a piece of machinery those implementations share.
//!
//! The one piece worth knowing about is [`Hole`]: a marker node plus whatever content currently
//! sits before it. Every view whose content can be replaced is built on one, and the marker is
//! why an emptied conditional still knows where to put the content that replaces it.

mod anchor;
mod any;
mod children;
mod either;
mod hole;
mod list;
mod option;
mod reactive;
mod scope;
mod text;
mod tuple;
#[allow(clippy::module_inception)]
mod view;

pub use crate::view::anchor::{Anchor, AnyAnchor, Empty};
pub use crate::view::any::{AnyView, AnyViewState};
pub use crate::view::children::{Children, ChildrenFn};
pub use crate::view::either::{Either, EitherState};
pub use crate::view::hole::Hole;
pub use crate::view::list::ListState;
pub use crate::view::option::OptionState;
pub use crate::view::reactive::ReactiveState;
pub use crate::view::scope::{Scoped, ScopedState};
pub use crate::view::text::TextState;
pub use crate::view::view::{IntoView, View};
