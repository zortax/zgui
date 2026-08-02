//! Which elements can be focused, and in which order sequential navigation reaches them.

use zgui_dom::{DocumentStore, Node, NodeIndex, NodeKey, NodeKind};
use zgui_layout::LayoutStore;
use zgui_vocab::UiState;

/// The attribute an element declares its focusability with.
pub const TABINDEX: &str = "tabindex";

/// The vocabulary elements that can be focused without declaring anything.
///
/// A control is operated, a field is filled in and an editor is typed into; all three are useless
/// to a keyboard user that cannot reach them, and requiring every one of them to be given a
/// `tabindex` would make forgetting one the default.
pub const FOCUSABLE_BY_NATURE: [&str; 3] = ["control", "field", "editor"];

/// Which way sequential focus navigation is moving.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FocusDirection {
    /// To the first element of the sequence.
    First,
    /// To the last element of the sequence.
    Last,
    /// To the element after the one that has focus now.
    Next,
    /// To the element before the one that has focus now.
    Prev,
}

/// Whether `node` can be focused at all.
///
/// `layout` decides the two questions the document cannot answer on its own: an element that
/// generates no box is not on the screen and cannot be focused, and neither is one whose box is
/// hidden. Passing [`None`] answers those two as "not laid out yet", which is what a query made
/// before the first frame has to mean — every other part of the rule still applies.
pub fn is_focusable(store: &DocumentStore, layout: Option<&LayoutStore>, node: NodeKey) -> bool {
    let Some(index) = store.index_of(node) else {
        return false;
    };
    let record = store.core(index);
    if record.kind() != NodeKind::Element {
        return false;
    }
    if record.ui_state().contains(UiState::DISABLED) {
        return false;
    }
    if !declares_focusable(store, index) {
        return false;
    }
    match layout {
        None => true,
        Some(layout) => is_displayed(layout, node),
    }
}

/// This element's `tabindex`, if it has one that parses.
///
/// An unparseable value is no `tabindex` at all rather than a zero: a typo that silently made an
/// element a tab stop would be worse than one that did nothing.
pub fn tabindex(store: &DocumentStore, index: NodeIndex) -> Option<i32> {
    Node::new(store.core(index))
        .attr(TABINDEX)
        .and_then(|value| value.as_str().trim().parse::<i32>().ok())
}

/// Every focusable element in `root`'s subtree, in sequential focus-navigation order.
///
/// A snapshot rather than an iterator, and deliberately: the source of truth is a document that
/// can be changed from inside anything this list is handed to, and a borrow held across arbitrary
/// component code is how that becomes a re-entrancy bug.
///
/// `root` itself is included when it is focusable, which is what makes a trap's own root a
/// candidate for the focus it moves inwards.
pub fn focusables(
    store: &DocumentStore,
    layout: Option<&LayoutStore>,
    root: NodeKey,
) -> Vec<NodeKey> {
    let mut found: Vec<(i32, usize, NodeKey)> = Vec::new();
    let Some(index) = store.index_of(root) else {
        return Vec::new();
    };
    collect(store, layout, index, &mut found);
    // A positive `tabindex` is a queue-jump: those elements come first, in increasing order, and
    // everything else keeps the order it appears in. Ties inside either group are broken by
    // document order, which is what the second component of the key is.
    found.sort_by_key(|(declared, position, _)| {
        let group = if *declared > 0 { 0 } else { 1 };
        (group, if *declared > 0 { *declared } else { 0 }, *position)
    });
    found
        .into_iter()
        .filter(|(declared, _, _)| *declared >= 0)
        .map(|(_, _, key)| key)
        .collect()
}

/// The element sequential navigation reaches from `current`.
///
/// `wrap` is what a focus trap turns on: past the last element it goes back to the first instead
/// of answering with nothing. Answers with nothing when the sequence is empty, and — with wrapping
/// off — when there is nothing beyond the current element in that direction.
pub fn step(
    sequence: &[NodeKey],
    current: Option<NodeKey>,
    direction: FocusDirection,
    wrap: bool,
) -> Option<NodeKey> {
    if sequence.is_empty() {
        return None;
    }
    let last = sequence.len() - 1;
    let at = current.and_then(|node| sequence.iter().position(|key| *key == node));
    let position = match (direction, at) {
        (FocusDirection::First, _) => 0,
        (FocusDirection::Last, _) => last,
        // Nothing in the sequence has focus, so moving forwards enters at the start and moving
        // backwards enters at the end — which is what tabbing into a trap from outside does.
        (FocusDirection::Next, None) => 0,
        (FocusDirection::Prev, None) => last,
        (FocusDirection::Next, Some(at)) if at < last => at + 1,
        (FocusDirection::Next, Some(_)) if wrap => 0,
        (FocusDirection::Prev, Some(at)) if at > 0 => at - 1,
        (FocusDirection::Prev, Some(_)) if wrap => last,
        _ => return None,
    };
    sequence.get(position).copied()
}

/// Whether this element declares itself focusable, by attribute or by being one of the names that
/// are focusable by nature.
fn declares_focusable(store: &DocumentStore, index: NodeIndex) -> bool {
    if tabindex(store, index).is_some() {
        return true;
    }
    let name = store.core(index).local_name().as_str();
    FOCUSABLE_BY_NATURE.contains(&name)
}

/// Whether this element generates a visible box.
fn is_displayed(layout: &LayoutStore, node: NodeKey) -> bool {
    let boxes = layout.boxes_of(node);
    if boxes.is_empty() {
        return false;
    }
    boxes.iter().any(|key| {
        layout.get(*key).is_some_and(|box_| {
            box_.style.get_inherited_box().visibility
                == zgui_css::values::size::VisibilityValue::Visible
        })
    })
}

/// Collects the focusable elements of one subtree, in document order.
fn collect(
    store: &DocumentStore,
    layout: Option<&LayoutStore>,
    index: NodeIndex,
    out: &mut Vec<(i32, usize, NodeKey)>,
) {
    let record = store.core(index);
    if record.kind() == NodeKind::Element {
        let key = record.key();
        if is_focusable(store, layout, key) {
            let declared = tabindex(store, index).unwrap_or(0);
            out.push((declared, out.len(), key));
        }
    }
    let mut child = record.first_child();
    while let Some(current) = child {
        collect(store, layout, current, out);
        child = store.core(current).next_sibling();
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeIndex, NodeKey};
    use zgui_interned::{AttrName, ElementName};
    use zgui_vocab::{SharedString, UiState};

    use super::{FocusDirection, focusables, is_focusable, step, tabindex};

    /// Appends an element with an optional `tabindex`.
    fn child(document: &Document, parent: NodeIndex, name: &str, index: Option<&str>) -> NodeIndex {
        document
            .edit(&EverythingMatters, |edit| {
                let node = edit.create_element(ElementName::new(name));
                edit.insert_before(parent, node, None);
                if let Some(value) = index {
                    edit.set_attribute(
                        node,
                        AttrName::new("tabindex"),
                        Some(SharedString::from(value)),
                    );
                }
                node
            })
            .expect("not poisoned")
    }

    /// A document with one root element, through the batch.
    fn rooted() -> (Document, NodeIndex) {
        let document = Document::new();
        let root = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                root
            })
            .expect("not poisoned");
        (document, root)
    }

    #[test]
    fn the_vocabulary_elements_that_are_focusable_by_nature_need_no_attribute() {
        let (document, root) = rooted();
        let control = child(&document, root, "control", None);
        let plain = child(&document, root, "box", None);
        let store = document.store();

        assert!(is_focusable(store, None, store.key_of(control)));
        assert!(!is_focusable(store, None, store.key_of(plain)));
    }

    #[test]
    fn a_disabled_element_is_not_focusable_however_it_was_declared() {
        let (document, root) = rooted();
        let control = child(&document, root, "control", Some("0"));
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_state(control, UiState::DISABLED, true);
            })
            .expect("not poisoned");

        let store = document.store();
        assert!(!is_focusable(store, None, store.key_of(control)));
        assert!(focusables(store, None, store.key_of(root)).is_empty());
    }

    #[test]
    fn a_positive_tabindex_jumps_the_queue_and_a_negative_one_leaves_it() {
        let (document, root) = rooted();
        let first = child(&document, root, "control", None);
        let jumped = child(&document, root, "box", Some("2"));
        let jumped_first = child(&document, root, "box", Some("1"));
        let skipped = child(&document, root, "control", Some("-1"));
        let last = child(&document, root, "field", None);

        let store = document.store();
        let sequence = focusables(store, None, store.key_of(root));
        assert_eq!(
            sequence,
            vec![
                store.key_of(jumped_first),
                store.key_of(jumped),
                store.key_of(first),
                store.key_of(last),
            ]
        );
        assert!(
            is_focusable(store, None, store.key_of(skipped)),
            "a negative index is focusable, just not in the sequence"
        );
        assert!(!sequence.contains(&store.key_of(skipped)));
    }

    #[test]
    fn an_unparseable_tabindex_is_no_declaration_at_all() {
        let (document, root) = rooted();
        let typo = child(&document, root, "box", Some("yes"));
        let store = document.store();
        assert_eq!(tabindex(store, typo), None);
        assert!(!is_focusable(store, None, store.key_of(typo)));
    }

    /// Three keys, standing in for a sequence.
    fn sequence() -> (Document, Vec<NodeKey>) {
        let (document, root) = rooted();
        let keys: Vec<NodeKey> = (0..3)
            .map(|_| {
                let node = child(&document, root, "control", None);
                document.store().key_of(node)
            })
            .collect();
        (document, keys)
    }

    #[test]
    fn stepping_past_the_end_stops_without_wrapping_and_comes_round_with_it() {
        let (_document, keys) = sequence();
        let last = Some(keys[2]);
        assert_eq!(step(&keys, last, FocusDirection::Next, false), None);
        assert_eq!(step(&keys, last, FocusDirection::Next, true), Some(keys[0]));
        assert_eq!(
            step(&keys, Some(keys[0]), FocusDirection::Prev, false),
            None
        );
        assert_eq!(
            step(&keys, Some(keys[0]), FocusDirection::Prev, true),
            Some(keys[2])
        );
    }

    #[test]
    fn entering_from_outside_enters_at_the_end_the_move_came_from() {
        let (_document, keys) = sequence();
        assert_eq!(
            step(&keys, None, FocusDirection::Next, false),
            Some(keys[0])
        );
        assert_eq!(
            step(&keys, None, FocusDirection::Prev, false),
            Some(keys[2])
        );
        assert_eq!(
            step(&keys, None, FocusDirection::First, false),
            Some(keys[0])
        );
        assert_eq!(
            step(&keys, None, FocusDirection::Last, false),
            Some(keys[2])
        );
        assert_eq!(step(&[], None, FocusDirection::First, true), None);
    }
}
