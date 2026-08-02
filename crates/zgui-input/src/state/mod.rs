//! The interaction state of a document, which lives in the document.
//!
//! `:hover`, `:active`, `:focus`, `:focus-visible` and `:focus-within` are written straight into
//! the elements they describe, and selector matching reads them there. There is deliberately no
//! parallel state machine: a second copy of "what is hovered" would be one more thing to keep in
//! step with the first, and every disagreement between them is a rule that does not fire.
//!
//! Every write here goes through the document's own batch, so a state change records what the
//! style engine needs, marks the path that has to be descended, and — for a bit no rule in the
//! document mentions — does none of that and costs nothing at all.
//!
//! ```
//! use zgui_dom::{Document, EverythingMatters};
//! use zgui_input::hit::HitChain;
//! use zgui_input::state::Interaction;
//! use zgui_interned::ElementName;
//! use zgui_vocab::UiState;
//!
//! let document = Document::new();
//! let (root, button) = document
//!     .edit(&EverythingMatters, |edit| {
//!         let root = edit.create_element(ElementName::new("root"));
//!         edit.insert_before(document.document_index(), root, None);
//!         let button = edit.create_element(ElementName::new("control"));
//!         edit.insert_before(root, button, None);
//!         (root, button)
//!     })
//!     .expect("not poisoned");
//! let chain = HitChain::to_root(document.store(), document.store().key_of(button));
//!
//! let mut interaction = Interaction::default();
//! interaction.hover.move_to(&document, &EverythingMatters, &chain);
//!
//! // The button is hovered, and so is everything it is inside.
//! assert!(document.store().core(button).ui_state().contains(UiState::HOVER));
//! assert!(document.store().core(root).ui_state().contains(UiState::HOVER));
//! ```

pub mod active;
pub mod focus;
pub mod hover;
pub mod within;

pub use crate::state::active::Active;
pub use crate::state::focus::{Focus, FocusSource};
pub use crate::state::hover::Hover;
pub use crate::state::within::{Moved, move_bit};

/// Everything the input system knows about how a document is being interacted with.
#[derive(Clone, Debug, Default)]
pub struct Interaction {
    /// What the pointer is over.
    pub hover: Hover,
    /// What is being pressed.
    pub active: Active,
    /// What has focus.
    pub focus: Focus,
}

#[cfg(test)]
mod tests;
