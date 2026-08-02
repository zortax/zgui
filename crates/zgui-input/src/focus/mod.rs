//! What can be focused, in what order, and what confines it.
//!
//! This is public because a component library cannot be written without it. A modal dialog has to
//! be able to say "tab stays inside me", a toolbar has to be able to say "the arrow keys move
//! between my buttons and tab treats us as one stop", and a menu has to be able to put focus back
//! where it came from when it closes. None of those is expressible from outside without the two
//! answers here: **which elements are focusable, in sequential order**, and **which subtree
//! traversal is confined to**.
//!
//! # Focusability is a stated rule, not an emergent one
//!
//! An element is focusable when it says so — a `tabindex` attribute — or when it is one of the
//! vocabulary elements that are focusable by nature, and when nothing about it makes it
//! unreachable: it is not disabled, it generates a box, and that box is visible. All four are
//! checked by [`order::is_focusable`], and a rule that is written down is a rule a component can
//! rely on.
//!
//! **Confinement is a separate question, and it is deliberately not folded into that one.** Whether
//! a trap is in force is a property of the window rather than of the element, so
//! [`order::is_focusable`] does not know about traps and never answers `false` because of one. A
//! caller moving focus asks both: [`order::focusables`] over
//! [`FocusTraps::confined_to`](trap::FocusTraps::confined_to) — or the whole document when nothing
//! is installed — for the sequence, and
//! [`FocusTraps::confines`](trap::FocusTraps::confines) for a destination it did not take from that
//! sequence. Asking only the first of the two is how focus leaves a modal.
//!
//! # Sequential order is `tabindex` and then document order
//!
//! Elements with a positive `tabindex` come first, in increasing order, and then everything else
//! in the order it appears in the document. A negative `tabindex` means focusable but not in the
//! sequence, which is what a roving toolbar sets on the items it is not currently pointing at.
//!
//! ```
//! use zgui_dom::{Document, EverythingMatters};
//! use zgui_input::focus::{FocusDirection, order};
//! use zgui_interned::ElementName;
//!
//! let document = Document::new();
//! let (root, first, second) = document
//!     .edit(&EverythingMatters, |edit| {
//!         let root = edit.create_element(ElementName::new("root"));
//!         edit.insert_before(document.document_index(), root, None);
//!         let first = edit.create_element(ElementName::new("control"));
//!         edit.insert_before(root, first, None);
//!         let second = edit.create_element(ElementName::new("control"));
//!         edit.insert_before(root, second, None);
//!         (root, first, second)
//!     })
//!     .expect("not poisoned");
//!
//! let store = document.store();
//! let sequence = order::focusables(store, None, store.key_of(root));
//! assert_eq!(sequence, vec![store.key_of(first), store.key_of(second)]);
//!
//! // And moving forwards from the first reaches the second.
//! let next = order::step(&sequence, Some(store.key_of(first)), FocusDirection::Next, true);
//! assert_eq!(next, Some(store.key_of(second)));
//! ```

pub mod order;
pub mod trap;

pub use crate::focus::order::FocusDirection;
pub use crate::focus::trap::{FocusTrapId, FocusTraps, TrapOptions};
pub use crate::state::focus::FocusSource;
