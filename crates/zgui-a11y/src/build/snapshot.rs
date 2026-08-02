//! The last update's nodes, kept so the next one can be a difference.
//!
//! An accesskit node is replace-not-patch: an update carries whole nodes, and a field left out of
//! one that is sent is a field cleared. There is therefore no such thing as a partial node to
//! hand-write, and the only honest way to produce "what changed" is to project the whole node again
//! and compare it with the one that was last sent.

use std::collections::{BTreeSet, HashMap};

use accesskit::{Node, NodeId, Rect};
use zgui_dom::NodeKey;
use zgui_scene::SpatialId;

use crate::id::to_a11y;
use crate::project::relations;

/// Every node the consumer is currently holding, as this side last sent it.
#[derive(Default)]
pub struct Snapshots {
    /// The nodes, by the document's own name for them.
    nodes: HashMap<NodeKey, Node>,
    /// Which held nodes were measured through which coordinate system.
    ///
    /// This is where the obligation to re-measure a node that was carried somewhere else lives, and
    /// it is here rather than in the walk that composes fragments because that walk is the thing
    /// this has to survive. A moved box is noticed today only because recomposing its fragments
    /// happens to pass over it; a coordinate system that is *written* rather than rebuilt moves
    /// every rectangle under it while touching no fragment at all, and nothing on that path would
    /// ever say so. A name for a coordinate system does not change when the matrix under it does,
    /// so the way back from a name to what was published through it has to be kept.
    ///
    /// Only nodes that declare something are indexed, for the same reason
    /// [`FrameDirty::is_semantic`](zgui_layout::fragment::diff::FrameDirty::is_semantic) exists: a
    /// list of five thousand plain rows inside a panel that slides moves five thousand boxes and
    /// changes nothing an assistive technology was told.
    by_space: HashMap<SpatialId, BTreeSet<NodeKey>>,
    /// Which coordinate systems each held node is filed under, so it can be taken out again.
    spaces_of: HashMap<NodeKey, Vec<SpatialId>>,
    /// Which held nodes name which other node, kept the other way round.
    ///
    /// A relation is written by the node that declares it and resolved by the consumer against the
    /// tree as a whole, so a target leaving the tree invalidates a node that did not itself change
    /// and is therefore in nothing this frame marked. Without a way back from a target to the nodes
    /// naming it, that node is never re-sent and the consumer is left holding an identifier it
    /// resolves with an unchecked lookup.
    referrers: HashMap<NodeId, BTreeSet<NodeKey>>,
}

impl Snapshots {
    /// Nothing sent yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many nodes the consumer is holding.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the consumer is holding nothing, which is what a fresh connection amounts to.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The children `node` was last sent with.
    pub fn children_of(&self, node: NodeKey) -> &[NodeId] {
        self.nodes
            .get(&node)
            .map_or(&[][..], accesskit::Node::children)
    }

    /// Records `projected` as sent, answering whether it differs from what was sent before.
    ///
    /// The answer is the whole decision: a node that projects to what the consumer already holds is
    /// not put in the update, which is what keeps an update proportional to what changed rather
    /// than to the size of the document.
    pub fn record(&mut self, node: NodeKey, projected: &Node) -> bool {
        let changed = !matches!(self.nodes.get(&node), Some(held) if held == projected);
        if changed {
            self.unindex(node);
            for target in relations::targets_of(projected) {
                self.referrers.entry(target).or_default().insert(node);
            }
            self.nodes.insert(node, projected.clone());
        }
        changed
    }

    /// Whether the consumer is holding a node for `node`.
    pub fn holds(&self, node: NodeKey) -> bool {
        self.nodes.contains_key(&node)
    }

    /// Files `node` under every coordinate system its published rectangle was measured through.
    ///
    /// Replaces whatever it was filed under, because a box that has changed coordinate system has
    /// not moved within the one it was in — it has left it, and a name still holding it would
    /// report it moved for a matrix its rectangle no longer depends on.
    ///
    /// Nothing is filed for a node the consumer is not holding: there is no rectangle to correct.
    pub fn measured_through(&mut self, node: NodeKey, spaces: impl Iterator<Item = SpatialId>) {
        self.unfile(node);
        if !self.nodes.contains_key(&node) {
            return;
        }
        let mut filed: Vec<SpatialId> = Vec::new();
        for space in spaces {
            if filed.contains(&space) {
                continue;
            }
            filed.push(space);
            self.by_space.entry(space).or_default().insert(node);
        }
        if filed.is_empty() {
            return;
        }
        self.spaces_of.insert(node, filed);
    }

    /// Every held node whose rectangle was measured through `space`.
    pub fn measured_in(&self, space: SpatialId) -> impl Iterator<Item = NodeKey> + '_ {
        self.by_space
            .get(&space)
            .into_iter()
            .flat_map(|nodes| nodes.iter().copied())
    }

    /// Takes `node` out of every coordinate system it was filed under.
    fn unfile(&mut self, node: NodeKey) {
        let Some(spaces) = self.spaces_of.remove(&node) else {
            return;
        };
        for space in spaces {
            let Some(nodes) = self.by_space.get_mut(&space) else {
                continue;
            };
            nodes.remove(&node);
            if nodes.is_empty() {
                self.by_space.remove(&space);
            }
        }
    }

    /// Rewrites the rectangle of the node the consumer is holding, answering with what to send.
    ///
    /// This is the whole of what a node that only moved owes, and it arrives at the same value a
    /// projection would: `bounds` is measured from the fragment tree by the same call
    /// [`geometry::apply`](crate::project::geometry::apply) makes, so the node this hands back is
    /// the node a full projection would have produced — reached without deriving the role, the
    /// name, the relations, the actions and the child list that a move cannot have touched.
    ///
    /// The reverse index of relations is deliberately left alone. A rectangle is not a relation, so
    /// a node whose rectangle changed names exactly the nodes it named before.
    ///
    /// Answers `None` when the consumer is not holding the node at all, and when it is already
    /// holding this rectangle.
    pub fn remeasure(&mut self, node: NodeKey, bounds: Option<Rect>) -> Option<Node> {
        let held = self.nodes.get_mut(&node)?;
        if held.bounds() == bounds {
            return None;
        }
        match bounds {
            Some(rect) => held.set_bounds(rect),
            None => held.clear_bounds(),
        }
        Some(held.clone())
    }

    /// Forgets `node`, which is no longer in the projected tree.
    pub fn forget(&mut self, node: NodeKey) {
        self.unindex(node);
        self.unfile(node);
        self.nodes.remove(&node);
    }

    /// Every held node that names `target`.
    ///
    /// What a departure has to re-project: the nodes here still carry a relation into a subtree the
    /// consumer is about to drop.
    pub fn referrers_of(&self, target: NodeId) -> Vec<NodeKey> {
        self.referrers
            .get(&target)
            .map_or_else(Vec::new, |set| set.iter().copied().collect())
    }

    /// Forgets everything, so the next update has to be a whole tree.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.referrers.clear();
        self.by_space.clear();
        self.spaces_of.clear();
    }

    /// Takes `node` out of the reverse index of everything it currently names.
    fn unindex(&mut self, node: NodeKey) {
        let Some(held) = self.nodes.get(&node) else {
            return;
        };
        for target in relations::targets_of(held) {
            let Some(set) = self.referrers.get_mut(&target) else {
                continue;
            };
            set.remove(&node);
            if set.is_empty() {
                self.referrers.remove(&target);
            }
        }
    }

    /// Every identifier the consumer is holding.
    pub fn ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.keys().copied().map(to_a11y)
    }
}

#[cfg(test)]
mod tests {
    use accesskit::{Node, Role};
    use zgui_dom::{Document, NodeKind};
    use zgui_interned::ElementName;

    use super::Snapshots;

    /// One live key of a throwaway document.
    fn key() -> (Document, zgui_dom::NodeKey) {
        let mut document = Document::new();
        let node = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let key = document.store().key_of(node);
        (document, key)
    }

    #[test]
    fn a_node_that_projects_to_what_was_already_sent_is_not_sent_again() {
        let (_document, key) = key();
        let mut snapshots = Snapshots::new();
        let node = Node::new(Role::Button);

        assert!(snapshots.record(key, &node), "the first one is a change");
        assert!(
            !snapshots.record(key, &node),
            "an update carrying nodes nothing can tell apart is an update that costs the \
             consumer work and tells it nothing"
        );
    }

    #[test]
    fn a_changed_node_is_sent_and_becomes_the_new_baseline() {
        let (_document, key) = key();
        let mut snapshots = Snapshots::new();
        snapshots.record(key, &Node::new(Role::Button));

        let mut changed = Node::new(Role::Button);
        changed.set_label("Save");
        assert!(snapshots.record(key, &changed));
        assert!(!snapshots.record(key, &changed));
    }
}
