//! Which animating elements `display: none` takes out of the rendering.
//!
//! An element with `display: none`, or below one, has no box. The restyle that hides it drops the
//! styles of everything below it, and the animation-only traversal does not descend past it. So
//! the element's animations and transitions end, and an animation mark on it has nothing to
//! restyle: the ordinary traversal styles it from its rules when it is shown again.

use style::dom::{TElement, TNode};
use zgui_dom::{Document, Node};

/// Whether `display: none` hides `node` in the document.
///
/// True for an element without a style and for an element that computes `display: none` or has a
/// styled ancestor that does. False for a node outside the document, which keeps its animations
/// while it is out.
pub(crate) fn is_hidden(document: &Document, node: Node<'_>) -> bool {
    let Some(root) = document.root() else {
        return false;
    };
    let Some(element) = node.as_element() else {
        return false;
    };
    let mut hidden = element.borrow_data().is_none_or(|data| !data.has_styles());
    let mut top = element;
    loop {
        hidden = hidden
            || top
                .borrow_data()
                .is_some_and(|data| data.has_styles() && data.styles.is_display_none());
        match TElement::traversal_parent(&top) {
            Some(parent) => top = parent,
            None => break,
        }
    }
    hidden && top.index() == root.index()
}
