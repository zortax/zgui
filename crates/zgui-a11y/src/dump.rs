//! An update as diffable text.
//!
//! A screenshot cannot show a missing relation and a `Debug` rendering of an accesskit node is a
//! property bag in whatever order the properties were set. Both make a regression in the one thing
//! this crate exists to produce invisible in review. This is the third option: a stable, sorted,
//! line-per-node rendering that a checked-in file can be compared against.
//!
//! Identifiers are rewritten as small ordinals in the order the nodes appear, because the real ones
//! carry an arena generation that changes with the order a test happened to build its document in.
//! Two updates that say the same thing therefore dump the same text.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use accesskit::{Node, NodeId, TreeUpdate};

use crate::project::relations;

/// `update` rendered as text, one node per line, sorted by identifier.
///
/// ```
/// use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};
///
/// let mut root = Node::new(Role::Window);
/// root.set_children(vec![NodeId(2)]);
/// let mut button = Node::new(Role::Button);
/// button.set_label("Save");
///
/// let update = TreeUpdate {
///     nodes: vec![(NodeId(1), root), (NodeId(2), button)],
///     tree: Some(Tree::new(NodeId(1))),
///     tree_id: TreeId::ROOT,
///     focus: NodeId(2),
/// };
/// let text = zgui_a11y::dump(&update);
/// assert!(text.contains("focus #2"));
/// assert!(text.contains("Button label=\"Save\""));
/// ```
pub fn dump(update: &TreeUpdate) -> String {
    let ordinals = ordinals(update);
    let name = |id: NodeId| match ordinals.get(&id) {
        Some(ordinal) => format!("#{ordinal}"),
        None => "#?".to_owned(),
    };

    let mut lines: Vec<(u32, String)> = Vec::new();
    for (id, node) in &update.nodes {
        let ordinal = ordinals.get(id).copied().unwrap_or(u32::MAX);
        lines.push((ordinal, line(node, &name)));
    }
    lines.sort_by_key(|(ordinal, _)| *ordinal);

    let mut text = String::new();
    if let Some(tree) = &update.tree {
        let _ = writeln!(text, "tree root={}", name(tree.root));
    }
    let _ = writeln!(text, "focus {}", name(update.focus));
    for (ordinal, rendered) in lines {
        let _ = writeln!(text, "#{ordinal} {rendered}");
    }
    text
}

/// A small number per identifier, assigned in the order the update lists its nodes.
fn ordinals(update: &TreeUpdate) -> BTreeMap<NodeId, u32> {
    let mut ordinals = BTreeMap::new();
    let mut next = 1;
    let assign = |id: NodeId, ordinals: &mut BTreeMap<NodeId, u32>, next: &mut u32| {
        ordinals.entry(id).or_insert_with(|| {
            let ordinal = *next;
            *next += 1;
            ordinal
        });
    };
    if let Some(tree) = &update.tree {
        assign(tree.root, &mut ordinals, &mut next);
    }
    for (id, _) in &update.nodes {
        assign(*id, &mut ordinals, &mut next);
    }
    for (_, node) in &update.nodes {
        for child in node.children() {
            assign(*child, &mut ordinals, &mut next);
        }
        for target in relations::targets_of(node) {
            assign(target, &mut ordinals, &mut next);
        }
    }
    assign(update.focus, &mut ordinals, &mut next);
    ordinals
}

/// One node, as one line.
fn line(node: &Node, name: &impl Fn(NodeId) -> String) -> String {
    let mut rendered = format!("{:?}", node.role());
    if let Some(label) = node.label() {
        let _ = write!(rendered, " label={label:?}");
    }
    if let Some(value) = node.value() {
        let _ = write!(rendered, " value={value:?}");
    }
    if let Some(description) = node.description() {
        let _ = write!(rendered, " description={description:?}");
    }
    states(node, &mut rendered);
    named_relations(node, name, &mut rendered);
    if !node.children().is_empty() {
        let names: Vec<String> = node.children().iter().map(|id| name(*id)).collect();
        let _ = write!(rendered, " children=[{}]", names.join(" "));
    }
    rendered
}

/// The states worth putting in a transcript: the ones a consumer announces.
fn states(node: &Node, into: &mut String) {
    for (set, word) in [
        (node.is_disabled(), "disabled"),
        (node.is_required(), "required"),
        (node.is_modal(), "modal"),
        (node.is_hidden(), "hidden"),
        (node.is_read_only(), "read_only"),
        (node.clips_children(), "clips_children"),
    ] {
        if set {
            let _ = write!(into, " {word}");
        }
    }
    if let Some(expanded) = node.is_expanded() {
        let _ = write!(into, " expanded={expanded}");
    }
    if let Some(selected) = node.is_selected() {
        let _ = write!(into, " selected={selected}");
    }
    if let Some(toggled) = node.toggled() {
        let _ = write!(into, " toggled={toggled:?}");
    }
    if let Some(invalid) = node.invalid() {
        let _ = write!(into, " invalid={invalid:?}");
    }
    if let Some(popup) = node.has_popup() {
        let _ = write!(into, " has_popup={popup:?}");
    }
}

/// The relations, each named as the ordinal of what it points at.
fn named_relations(node: &Node, name: &impl Fn(NodeId) -> String, into: &mut String) {
    let lists = [
        ("labelled_by", node.labelled_by()),
        ("described_by", node.described_by()),
        ("controls", node.controls()),
        ("owns", node.owns()),
        ("radio_group", node.radio_group()),
    ];
    for (relation, targets) in lists {
        if targets.is_empty() {
            continue;
        }
        let names: Vec<String> = targets.iter().map(|id| name(*id)).collect();
        let _ = write!(into, " {relation}=[{}]", names.join(" "));
    }
    for (relation, target) in [
        ("active_descendant", node.active_descendant()),
        ("popup_for", node.popup_for()),
        ("error_message", node.error_message()),
    ] {
        if let Some(target) = target {
            let _ = write!(into, " {relation}={}", name(target));
        }
    }
}

#[cfg(test)]
mod tests {
    use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};

    use super::dump;

    #[test]
    fn the_same_tree_built_from_different_identifiers_dumps_the_same_text() {
        let render = |base: u64| {
            let mut root = Node::new(Role::Window);
            root.set_children(vec![NodeId(base + 1)]);
            let mut field = Node::new(Role::TextInput);
            field.set_labelled_by(vec![NodeId(base + 2)]);
            let mut label = Node::new(Role::Label);
            label.set_value("Name");
            dump(&TreeUpdate {
                nodes: vec![
                    (NodeId(base), root),
                    (NodeId(base + 1), field),
                    (NodeId(base + 2), label),
                ],
                tree: Some(Tree::new(NodeId(base))),
                tree_id: TreeId::ROOT,
                focus: NodeId(base + 1),
            })
        };
        assert_eq!(render(1_000), render(9_000_000));
    }

    #[test]
    fn a_relation_is_visible_in_the_text() {
        let text = {
            let mut field = Node::new(Role::TextInput);
            field.set_labelled_by(vec![NodeId(2)]);
            dump(&TreeUpdate {
                nodes: vec![(NodeId(1), field), (NodeId(2), Node::new(Role::Label))],
                tree: None,
                tree_id: TreeId::ROOT,
                focus: NodeId(1),
            })
        };
        assert!(
            text.contains("labelled_by=[#2]"),
            "a relation absent from the transcript is a relation a review cannot see: {text}"
        );
    }
}
