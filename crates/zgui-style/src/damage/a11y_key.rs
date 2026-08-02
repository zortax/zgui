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

use std::hash::{BuildHasher, Hasher};

use rustc_hash::FxBuildHasher;
use zgui_css::ComputedStyle;
use zgui_dom::side::a11y_key::A11yKey;
use zgui_dom::{DocumentStore, NodeIndex, NodeKind};

/// The accessibility key of the element at `index`.
pub fn a11y_key(store: &DocumentStore, index: NodeIndex, style: &ComputedStyle) -> A11yKey {
    A11yKey {
        // Visibility and the writing direction decide whether the element is exposed at all and
        // how its text runs are ordered, and both live in this one group.
        style: style.clone_inherited_box().heap_ptr() as usize,
        semantics: store
            .columns()
            .semantics
            .get(store.key_of(index))
            .as_ref()
            .map_or(0, |semantics| core::ptr::from_ref(&**semantics) as usize),
        content: content_hash(store, index),
    }
}

/// A hash of the text this element would be read out as.
///
/// Only the element's own text children participate. A name assembled from further down is the
/// projection's business, and every node on the way carries its own key: hashing a whole subtree
/// here would make one deep element pay for every text node beneath it on every restyle.
fn content_hash(store: &DocumentStore, index: NodeIndex) -> u64 {
    let mut hasher = FxBuildHasher.build_hasher();
    let mut child = store.core(index).first_child();
    while let Some(current) = child {
        if store.core(current).kind() == NodeKind::Text
            && let Some(text) = zgui_dom::text::node::text_of(store, current)
        {
            hasher.write(text.as_bytes());
            hasher.write_u8(0);
        }
        child = store.core(current).next_sibling();
    }
    hasher.finish()
}
