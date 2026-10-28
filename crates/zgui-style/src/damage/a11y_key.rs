//! The identity of everything one element's accessible description depends on.
//!
//! The same idea as the paint key one level over, with one part that cannot be an identity. A
//! style key can be a set of addresses because computed values are shared and immutable. An
//! accessible name is not: editing a text node changes what a screen reader would say without
//! changing any style group and without changing any semantics record, so identity alone would
//! report "nothing changed" for the change a user would most notice.
//!
//! This is not the whole accessibility predicate and does not try to be. Text, semantics and
//! value-bearing property writes mark the node directly as they happen, and the fragment diff
//! marks it again when its geometry moves, because an accessibility node's bounds are geometry.
//! What this covers is the third producer: a *style* change that alters whether or how the element
//! is exposed.

use zgui_css::ComputedStyle;
use zgui_dom::side::a11y_key::A11yKey;
use zgui_dom::{DocumentStore, NodeIndex};

/// The accessibility key of the element at `index`.
///
/// The key holds the semantics record alone. A style group's address moves on every cascade of an
/// element that declares anything in it, and a hash of the text is a walk over the document's text
/// on every restyle; the projection needs neither, because it reads no computed value and a text
/// edit marks the element itself.
pub fn a11y_key(store: &DocumentStore, index: NodeIndex, _style: &ComputedStyle) -> A11yKey {
    A11yKey {
        semantics: store
            .columns()
            .semantics
            .get(store.key_of(index))
            .as_ref()
            .map_or(0, |semantics| core::ptr::from_ref(&**semantics) as usize),
    }
}
