//! Where the keyboard is, and where it may go.
//!
//! Two behaviours, and between them they cover every composite control there is.
//!
//! [`FocusScope`] answers *may focus leave?* — a modal surface confines navigation to itself and
//! puts focus back where it came from when it closes.
//!
//! [`RovingFocus`] answers *what does the next arrow key do?* — a group of controls becomes one
//! tab stop with the arrow keys moving inside it, which is the pattern behind toolbars, tab bars,
//! menus, radio groups and listboxes.
//!
//! Both are written against the public focus surface — [`NodeRef::trap_focus`] and
//! [`NodeRef::focus`] — and neither knows anything about how focus is actually resolved.

pub mod roving;
pub mod scope;

pub use crate::focus::roving::{
    Orientation, RovingContext, RovingFocus, RovingFocusProps, RovingItem, use_roving_item,
    use_roving_item_when,
};
pub use crate::focus::scope::{FocusScope, FocusScopeProps};

#[allow(unused_imports)]
use zgui::prelude::NodeRef;
