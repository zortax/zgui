//! The flag that gets the animation-only traversal from the root down to one element.
//!
//! The animation-only traversal is not the ordinary one and does not descend for the same reasons.
//! It never propagates a selector-matching hint, so the only thing that takes it below an element
//! is a per-element flag saying "an animation-only restyle is pending somewhere under me". An
//! element carrying an animation hint and no such flag on any of its ancestors is unreachable: the
//! traversal starts at the root, asks the root that question, is told no, and stops — the element
//! never restyles, and the hint it is still carrying is what the *ordinary* traversal then refuses
//! to process.
//!
//! Raising the flag is therefore part of asking for an animation restyle, not a separate step a
//! caller may forget. It is cleared by the traversal that reads it, which is what makes storing it
//! safe: one storage, one retirement, inside one traversal.

use style::dom::TNode;
use zgui_dom::Node;

/// Records on every ancestor of `node` that an animation-only restyle is pending below it.
///
/// The chain is walked to the root rather than to the nearest styled ancestor, because the
/// traversal starts at the root: a chain broken anywhere above the element is a chain the descent
/// stops at.
pub fn raise_to_root(node: Node<'_>) {
    let mut ancestor = node.traversal_parent();
    while let Some(element) = ancestor {
        element.note_animation_work_below();
        ancestor = element.traversal_parent();
    }
}

/// Forgets that an animation-only restyle is pending below `element`.
///
/// Called by the animation-only traversal on each element it has finished visiting, after the
/// engine has read the flag to decide whether to descend and after it has noted the children it is
/// descending to. What is left is a flag nothing this frame will read again, and leaving it raised
/// would take the next frame's traversal down a subtree with nothing in it to do.
pub fn clear(element: Node<'_>) {
    element.clear_animation_work_below();
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, NodeKind};
    use zgui_interned::ElementName;

    use super::{clear, raise_to_root};

    #[test]
    fn the_flag_is_raised_on_every_ancestor_and_not_on_the_element_itself() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let middle = document.append(root, NodeKind::Element, ElementName::new("box"));
        let leaf = document.append(middle, NodeKind::Element, ElementName::new("box"));

        raise_to_root(document.node(leaf));

        assert!(
            document.node(root).has_animation_work_below(),
            "the traversal starts at the root, so a root without the flag reaches nothing"
        );
        assert!(document.node(middle).has_animation_work_below());
        assert!(
            !document.node(leaf).has_animation_work_below(),
            "the element's own hint is what makes it visited; a flag on it would claim work below \
             it that is not there"
        );

        clear(document.node(middle));
        assert!(!document.node(middle).has_animation_work_below());
    }
}
