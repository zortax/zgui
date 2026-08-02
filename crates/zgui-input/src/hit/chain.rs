//! The path an event travels: an element and its ancestors, root first.

use smallvec::SmallVec;
use zgui_dom::{DocumentStore, NodeKey};

/// An element and every ancestor of it, from the root of the document down to the element itself.
///
/// This is the path an event is delivered along and the path interaction state is written up. It
/// is built by walking the *document*, never the box tree, and the distinction is load-bearing:
/// an element that generates no box of its own — `display: contents` — is on this path and is not
/// on the box path, so a rule written against it keeps matching.
///
/// The chain is a snapshot. It names nodes by a key that carries a generation, so a chain held
/// across a mutation that removed one of its elements answers about that element with nothing
/// rather than about whatever took its slot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HitChain {
    /// Root first, target last.
    path: SmallVec<[NodeKey; 8]>,
}

impl HitChain {
    /// The chain from the document's root down to `node`.
    ///
    /// Text nodes and the document node itself are not on it: an event is delivered to elements,
    /// and a press on the text inside a button is a press on the button.
    pub fn to_root(store: &DocumentStore, node: NodeKey) -> Self {
        let mut path: SmallVec<[NodeKey; 8]> = SmallVec::new();
        let Some(mut index) = store.index_of(node) else {
            return Self::default();
        };
        loop {
            let record = store.core(index);
            if record.kind() == zgui_dom::NodeKind::Element {
                path.push(record.key());
            }
            match record.parent() {
                Some(parent) => index = parent,
                None => break,
            }
        }
        path.reverse();
        Self { path }
    }

    /// A chain over an explicit path, root first.
    ///
    /// For a caller that already knows the path — an event aimed at a particular element rather
    /// than at a point.
    pub fn from_path(path: impl IntoIterator<Item = NodeKey>) -> Self {
        Self {
            path: path.into_iter().collect(),
        }
    }

    /// The whole path, root first and target last.
    pub fn path(&self) -> &[NodeKey] {
        &self.path
    }

    /// The element the event was aimed at, which is the last of the path.
    pub fn target(&self) -> Option<NodeKey> {
        self.path.last().copied()
    }

    /// How many elements are on the path.
    pub fn depth(&self) -> usize {
        self.path.len()
    }

    /// Whether the path is empty, which is what a point over nothing answers with.
    pub fn is_empty(&self) -> bool {
        self.path.is_empty()
    }

    /// Whether `node` is on the path.
    pub fn contains(&self, node: NodeKey) -> bool {
        self.path.contains(&node)
    }

    /// The chain truncated at `node`, so that it ends there instead of at the original target.
    ///
    /// What a pointer capture needs: the pressed element keeps receiving the pointer wherever it
    /// goes, and the event still travels down through that element's own ancestors.
    pub fn truncated_at(&self, node: NodeKey) -> Self {
        match self.path.iter().position(|key| *key == node) {
            Some(position) => Self {
                path: self.path[..=position].iter().copied().collect(),
            },
            None => Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::Document;
    use zgui_interned::ElementName;

    use super::HitChain;

    /// A document of `root > middle > leaf`, with a text node under the leaf.
    fn nested() -> (Document, [zgui_dom::NodeKey; 4]) {
        let document = Document::new();
        let indices = document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let middle = edit.create_element(ElementName::new("middle"));
                edit.insert_before(root, middle, None);
                let leaf = edit.create_element(ElementName::new("leaf"));
                edit.insert_before(middle, leaf, None);
                let text = edit.create_text("hello");
                edit.insert_before(leaf, text, None);
                [root, middle, leaf, text]
            })
            .expect("not poisoned");
        let keys = indices.map(|index| document.store().key_of(index));
        (document, keys)
    }

    #[test]
    fn the_chain_runs_from_the_root_down_to_the_element() {
        let (document, [root, middle, leaf, _]) = nested();
        let chain = HitChain::to_root(document.store(), leaf);
        assert_eq!(chain.path(), &[root, middle, leaf]);
        assert_eq!(chain.target(), Some(leaf));
        assert_eq!(chain.depth(), 3);
    }

    #[test]
    fn a_text_node_dispatches_through_the_element_that_holds_it() {
        let (document, [root, middle, leaf, text]) = nested();
        let chain = HitChain::to_root(document.store(), text);
        assert_eq!(
            chain.path(),
            &[root, middle, leaf],
            "the text node itself is not on the path, and its element is the target"
        );
    }

    #[test]
    fn a_chain_to_a_node_that_is_gone_is_empty() {
        // A chain is built from a name that carries a generation, so a name held across the
        // removal of what it named answers with nothing rather than with whatever took its slot.
        let (mut document, [_, _, leaf, _]) = nested();
        let index = document.store().index_of(leaf).expect("a live node");
        document
            .edit(&zgui_dom::EverythingMatters, |edit| edit.remove(index))
            .expect("not poisoned");
        zgui_dom::arena::recycle::end_frame(&mut document);

        assert!(HitChain::to_root(document.store(), leaf).is_empty());
    }

    #[test]
    fn truncating_ends_the_chain_at_the_capturing_element() {
        let (document, [root, middle, leaf, _]) = nested();
        let chain = HitChain::to_root(document.store(), leaf);
        let held = chain.truncated_at(middle);
        assert_eq!(held.path(), &[root, middle]);
        assert!(held.contains(root));
        assert!(!held.contains(leaf));
        assert!(
            chain.truncated_at(root).truncated_at(leaf).is_empty(),
            "truncating at an element that is not on the path answers with no path at all"
        );
    }
}
