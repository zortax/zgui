//! The animations one document is running, held across frames.
//!
//! The set has to outlive a frame, and that is the whole reason this type exists. A transition is
//! created by comparing the cascade result before a change with the one after it — a comparison
//! only the frame in which the change happened can make — and then read on every frame until it
//! ends. A set rebuilt per frame would start every transition again from its beginning on every
//! frame, which is a value that never moves and a loop that never stops.

use style::animation::{AnimationSetKey, DocumentAnimationSet};
use style::dom::OpaqueNode;
use zgui_dom::NodeIndex;
use zgui_dom::id::opaque::{node_from_opaque, opaque_node};

use crate::driver::animations::AnimationTime;

/// Every animation and transition running in one document.
#[derive(Default)]
pub struct Animations {
    /// The engine's own table, shared with the cascade rather than copied into it.
    set: DocumentAnimationSet,
    /// The time the last tick advanced to.
    now: AnimationTime,
}

impl Animations {
    /// A document with nothing animating.
    pub fn new() -> Self {
        Self::default()
    }

    /// The time animations are currently being read at.
    pub fn now(&self) -> AnimationTime {
        self.now
    }

    /// Moves the clock the engine reads animated values at.
    pub fn set_now(&mut self, now: AnimationTime) {
        self.now = now;
    }

    /// Whether nothing at all is animating.
    ///
    /// Cheap enough to ask on every frame, and asking is what lets a document that is not animating
    /// skip the tick entirely rather than walking an empty table.
    pub fn is_empty(&self) -> bool {
        self.set.sets.read().is_empty()
    }

    /// How many animations and transitions are running on one element.
    ///
    /// This is what a component asking "is anything still animating here?" is answered from, and it
    /// counts only what is still to be ticked: a finished transition kept for one more frame so its
    /// end can be reported is not running.
    pub fn running_on(&self, node: NodeIndex) -> usize {
        self.set
            .sets
            .read()
            .get(&key_of(node))
            .map_or(0, |set| set.running_animation_and_transition_count())
    }

    /// Every element with something still to be ticked, by slot number.
    ///
    /// The order is the table's and therefore arbitrary; a caller that needs one sorts. What this
    /// is for is the frame that *created* an animation: the tick runs before the cascade, and the
    /// cascade is what starts a keyframe animation, so the elements it started are not in that
    /// frame's report and nothing else would ever ask the loop to come back for them.
    pub fn running_elements(&self) -> Vec<NodeIndex> {
        self.set
            .sets
            .read()
            .iter()
            .filter(|(_, set)| set.running_animation_and_transition_count() > 0)
            .map(|(key, _)| node_of(key))
            .collect()
    }

    /// The table itself, for the cascade to read and write as it styles the document.
    ///
    /// The value shares its storage with this one rather than copying it, which is what lets the
    /// traversal start a transition that the next frame's tick can then advance.
    pub(crate) fn shared(&self) -> DocumentAnimationSet {
        self.set.clone()
    }

    /// The table itself, for this crate's own tick.
    pub(crate) fn document_set(&self) -> &DocumentAnimationSet {
        &self.set
    }
}

/// The row one element's animations occupy.
pub(crate) fn key_of(node: NodeIndex) -> AnimationSetKey {
    AnimationSetKey::new_for_non_pseudo(OpaqueNode(opaque_node(node)))
}

/// The element a row belongs to.
pub(crate) fn node_of(key: &AnimationSetKey) -> NodeIndex {
    node_from_opaque(key.node.0)
}

#[cfg(test)]
mod tests {
    use zgui_dom::NodeIndex;

    use super::{Animations, key_of, node_of};

    #[test]
    fn a_row_names_the_element_it_was_made_for() {
        for index in [0u32, 1, 512, 100_000] {
            let node = NodeIndex::new(index);
            assert_eq!(node_of(&key_of(node)), node);
        }
    }

    #[test]
    fn a_document_with_nothing_animating_says_so() {
        let animations = Animations::new();
        assert!(animations.is_empty());
        assert_eq!(animations.running_on(NodeIndex::new(3)), 0);
    }

    #[test]
    fn the_shared_table_is_the_same_table() {
        // Not a copy: the cascade writes into the value handed to it, and this tick reads what it
        // wrote. A clone that deep-copied would lose every transition the frame started.
        let animations = Animations::new();
        let shared = animations.shared();
        shared
            .sets
            .write()
            .insert(key_of(NodeIndex::new(7)), Default::default());
        assert!(!animations.is_empty());
    }
}
