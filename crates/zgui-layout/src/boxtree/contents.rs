//! `display: contents`, which makes an element's children into its parent's children.
//!
//! The element generates no box of its own, so nothing about it can be painted, sized or clicked —
//! but its children keep theirs and take its place in the tree. Flattening happens while the tree
//! is built rather than while it is walked, because the walk runs inside the layout algorithms'
//! innermost loops and has to be free of both filtering and allocation.

use crate::boxtree::anonymous::Placed;

/// Splices a flattened element's children in where the element itself would have gone.
pub fn splice(out: &mut Vec<Placed>, flattened: Vec<Placed>) {
    out.extend(flattened);
}

/// Whether a run of children left nothing behind, which is what a flattened element with no
/// children produces and is not an error.
pub fn is_empty(children: &[Placed]) -> bool {
    children.is_empty()
}

#[cfg(test)]
mod tests {
    use zgui_arena::{DomainId, Generation};
    use zgui_dom::side::BoxKey;

    use crate::boxtree::anonymous::Placed;
    use crate::style::convert::display::Participation;

    use super::{is_empty, splice};

    fn placed(index: u32) -> Placed {
        Placed {
            key: BoxKey::new(index, Generation::FIRST, DomainId::FIRST),
            participation: Participation::Block,
            out_of_flow: false,
        }
    }

    #[test]
    fn a_flattened_elements_children_take_its_place_in_order() {
        let mut children = vec![placed(1)];
        splice(&mut children, vec![placed(2), placed(3)]);
        children.push(placed(4));
        assert_eq!(
            children
                .iter()
                .map(|child| child.key.index())
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn a_flattened_element_with_no_children_leaves_no_orphan() {
        let mut children: Vec<Placed> = Vec::new();
        splice(&mut children, Vec::new());
        assert!(is_empty(&children));
    }
}
