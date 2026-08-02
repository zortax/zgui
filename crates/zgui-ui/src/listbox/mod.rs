//! A list of options, one of which is chosen, driven from a control that is not in it.
//!
//! A select, a combobox and a command palette are the same control three times over. In all three
//! the keyboard stays on a trigger or a text field, the *options* are somewhere else entirely, and
//! the one being walked is named to a reader by [`active_descendant`] rather than by being focused
//! — because moving focus into the list would take the caret out of the field the user is typing
//! into.
//!
//! [`active_descendant`]: zgui::prelude::A11yBinding::active_descendant

mod catalogue;
mod keys;
mod labels;
mod option;
mod registry;

pub use crate::listbox::catalogue::{
    ListboxCatalogue, ListboxCatalogueOf, ListboxCatalogueOfProps,
};
pub use crate::listbox::keys::{ListboxAction, action_for};
pub use crate::listbox::labels::ListboxLabels;
pub use crate::listbox::option::{ListboxEntry, ListboxOption};
pub use crate::listbox::registry::{Listbox, use_listbox_option};
