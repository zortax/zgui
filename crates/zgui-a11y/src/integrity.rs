//! The one invariant an accessibility update must satisfy, checked rather than assumed.
//!
//! Every identifier an update mentions — a child, a label, a controlled panel, an owned pop-up —
//! must resolve either within that same update or within the tree the consumer is already holding.
//! One that does not is not a cosmetic defect: a consumer resolves relations with an unchecked
//! lookup, so a dangling identifier is a panic on a thread the application does not own and cannot
//! catch. `accesskit_consumer` does exactly that when it walks an explicit `labelled_by`.
//!
//! Checking it is therefore a crash guard, and it is cheap: one lookup per identifier mentioned.

use std::collections::HashSet;

use accesskit::{Node, NodeId, TreeUpdate};

use crate::project::relations;

/// An identifier an update mentions that nothing resolves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dangling {
    /// The node that mentioned it.
    pub from: NodeId,
    /// The identifier that resolves to nothing.
    pub to: NodeId,
    /// In what capacity it was mentioned.
    pub as_a: Mention,
}

/// How one node mentioned another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mention {
    /// As one of its children.
    Child,
    /// As one of its relations.
    Relation,
    /// As the node holding focus.
    Focus,
}

/// Every identifier in `update` that resolves neither within it nor within `retained`.
///
/// `retained` is what the consumer is already holding — [`A11yBuilder::retained`] answers it.
///
/// [`A11yBuilder::retained`]: crate::A11yBuilder::retained
///
/// ```
/// use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};
///
/// let mut root = Node::new(Role::Window);
/// root.set_children(vec![NodeId(2)]);
/// let update = TreeUpdate {
///     nodes: vec![(NodeId(1), root)],
///     tree: Some(Tree::new(NodeId(1))),
///     tree_id: TreeId::ROOT,
///     focus: NodeId(1),
/// };
///
/// // The child is named and never sent, and nothing is holding it.
/// let dangling = zgui_a11y::dangling(&update, std::iter::empty());
/// assert_eq!(dangling.len(), 1);
/// assert_eq!(dangling[0].to, NodeId(2));
/// ```
pub fn dangling(update: &TreeUpdate, retained: impl Iterator<Item = NodeId>) -> Vec<Dangling> {
    let mut resolvable: HashSet<NodeId> = retained.collect();
    for (id, _) in &update.nodes {
        resolvable.insert(*id);
    }

    let mut found = Vec::new();
    for (id, node) in &update.nodes {
        mentions(node, *id, &resolvable, &mut found);
    }
    if !resolvable.contains(&update.focus) {
        found.push(Dangling {
            from: update.focus,
            to: update.focus,
            as_a: Mention::Focus,
        });
    }
    found
}

/// Records every identifier `node` mentions that `resolvable` does not hold.
fn mentions(node: &Node, id: NodeId, resolvable: &HashSet<NodeId>, found: &mut Vec<Dangling>) {
    for child in node.children() {
        if !resolvable.contains(child) {
            found.push(Dangling {
                from: id,
                to: *child,
                as_a: Mention::Child,
            });
        }
    }
    for target in relations::targets_of(node) {
        if !resolvable.contains(&target) {
            found.push(Dangling {
                from: id,
                to: target,
                as_a: Mention::Relation,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};

    use super::{Mention, dangling};

    /// An update carrying `nodes`, rooted and focused at the first of them.
    fn update(nodes: Vec<(NodeId, Node)>) -> TreeUpdate {
        let root = nodes.first().expect("at least one node").0;
        TreeUpdate {
            nodes,
            tree: Some(Tree::new(root)),
            tree_id: TreeId::ROOT,
            focus: root,
        }
    }

    #[test]
    fn an_update_that_names_only_what_it_carries_is_clean() {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![NodeId(2)]);
        let child = Node::new(Role::Button);
        assert!(
            dangling(
                &update(vec![(NodeId(1), root), (NodeId(2), child)]),
                std::iter::empty()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_relation_into_the_retained_tree_is_clean() {
        let mut field = Node::new(Role::TextInput);
        field.set_labelled_by(vec![NodeId(9)]);
        let mut root = Node::new(Role::Window);
        root.set_children(vec![NodeId(2)]);
        let update = update(vec![(NodeId(1), root), (NodeId(2), field)]);
        assert!(dangling(&update, [NodeId(9)].into_iter()).is_empty());
    }

    #[test]
    fn a_relation_into_nothing_is_reported_rather_than_sent() {
        // The positive control for the guard: with the target absent, the check has to fire, or it
        // is a check that would pass while the crash it exists to stop is being shipped.
        let mut field = Node::new(Role::TextInput);
        field.set_labelled_by(vec![NodeId(9)]);
        let mut root = Node::new(Role::Window);
        root.set_children(vec![NodeId(2)]);
        let update = update(vec![(NodeId(1), root), (NodeId(2), field)]);

        let found = dangling(&update, std::iter::empty());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].to, NodeId(9));
        assert_eq!(found[0].as_a, Mention::Relation);
    }

    #[test]
    fn a_focus_nothing_resolves_is_reported() {
        let root = Node::new(Role::Window);
        let mut update = update(vec![(NodeId(1), root)]);
        update.focus = NodeId(77);
        let found = dangling(&update, std::iter::empty());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].as_a, Mention::Focus);
    }
}
