//! Which nodes owe an accessibility rebuild, gathered from the document's own marks.
//!
//! # Why the marks are drained even when nobody is listening
//!
//! The invalidation lattice is a union: a bit left set on a node keeps every ancestor's subtree
//! union set as well, and a stage that never drains its phase leaves the whole document permanently
//! marked — every other stage then descends everywhere on every frame. On a machine with no
//! assistive technology running, the tree is never built; the *marks* must still be retired, or the
//! cost of accessibility is paid in full by the machines that use none of it.
//!
//! So collecting and building are two steps. Collecting always happens. Building happens when
//! something is listening, and consumes whatever collecting has gathered since it last ran.

use std::collections::BTreeSet;

use zgui_bits::Dirty;
use zgui_dom::{Document, NodeKey};

/// How many nodes may accumulate unbuilt before a whole tree is cheaper than a difference.
///
/// Reached only when nothing has been listening for a long time, which is the case where the
/// difference would be discarded anyway.
const BEFORE_A_FULL_REBUILD_IS_CHEAPER: usize = 4096;

/// The nodes whose accessibility projection is owed, and whether that is now everything.
#[derive(Debug, Default)]
pub struct Pending {
    /// The nodes owed, in a stable order so that two runs of the same program agree.
    nodes: BTreeSet<NodeKey>,
    /// The nodes that only moved, which owe a rectangle rather than a projection.
    ///
    /// Held apart from [`Pending::nodes`] because the two are answered differently and one is very
    /// much cheaper. A node is never in both: a projection subsumes a rectangle, so a node told to
    /// project drops whatever move it was carrying.
    moved: BTreeSet<NodeKey>,
    /// Whether so much is owed that the whole tree should be rebuilt instead.
    everything: bool,
}

impl Pending {
    /// Nothing owed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Drains `document`'s accessibility marks into this set, retiring the phase.
    ///
    /// Returns how many nodes the walk visited, which is what a budget assertion measures.
    pub fn collect(&mut self, document: &mut Document) -> usize {
        // From the document node rather than from the root element: the union is carried by every
        // ancestor, and a walk that started below the document node would leave the topmost one
        // marked for ever — which is a subtree nothing else can ever skip.
        let root = document.document_index();
        let mut visited = 0;
        let mut found = Vec::new();
        zgui_dom::dirty::walk::walk(
            document.store_mut(),
            root,
            Dirty::A11Y,
            &mut |store, index| {
                visited += 1;
                found.push(store.key_of(index));
            },
        );
        for key in found {
            self.mark(key);
        }
        visited
    }

    /// Records that `node` owes a projection.
    pub fn mark(&mut self, node: NodeKey) {
        if self.everything {
            return;
        }
        self.moved.remove(&node);
        self.nodes.insert(node);
        if self.owed_len() > BEFORE_A_FULL_REBUILD_IS_CHEAPER {
            self.demand_everything();
        }
    }

    /// Records that `node`'s boxes were carried somewhere else and nothing else about it changed.
    ///
    /// Ignored for a node that already owes a projection, because projecting it again answers where
    /// it is along with everything else.
    pub fn mark_moved(&mut self, node: NodeKey) {
        if self.everything || self.nodes.contains(&node) {
            return;
        }
        self.moved.insert(node);
        if self.owed_len() > BEFORE_A_FULL_REBUILD_IS_CHEAPER {
            self.demand_everything();
        }
    }

    /// Records that the whole tree is owed.
    pub fn demand_everything(&mut self) {
        self.everything = true;
        self.nodes.clear();
        self.moved.clear();
    }

    /// Whether anything at all is owed.
    pub fn is_owed(&self) -> bool {
        self.everything || !self.nodes.is_empty() || !self.moved.is_empty()
    }

    /// How many nodes are named, of either kind.
    fn owed_len(&self) -> usize {
        self.nodes.len() + self.moved.len()
    }

    /// Whether what is owed is the whole tree.
    pub fn is_everything(&self) -> bool {
        self.everything
    }

    /// The nodes owed a projection, in document-key order.
    pub fn nodes(&self) -> impl Iterator<Item = NodeKey> + '_ {
        self.nodes.iter().copied()
    }

    /// The nodes owed only a rectangle, in document-key order.
    pub fn moved(&self) -> impl Iterator<Item = NodeKey> + '_ {
        self.moved.iter().copied()
    }

    /// Empties the set, the work having been done.
    pub fn take(&mut self) -> Self {
        core::mem::take(self)
    }
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_dom::{Document, EverythingMatters, NodeKind};
    use zgui_interned::ElementName;

    use super::Pending;

    #[test]
    fn collecting_retires_the_phase_so_a_machine_with_no_screen_reader_pays_nothing_twice() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let text = document.append(root, NodeKind::Text, ElementName::new("#text"));
        document
            .edit(&EverythingMatters, |edit| edit.set_text(text, "one"))
            .expect("not poisoned");

        let mut pending = Pending::new();
        assert!(pending.collect(&mut document) > 0);
        assert!(pending.is_owed());

        let mut again = Pending::new();
        assert_eq!(
            again.collect(&mut document),
            0,
            "a phase that is never drained leaves every ancestor's subtree union set for ever"
        );
        assert!(
            !document
                .store()
                .core(document.document_index())
                .dirty()
                .subtree()
                .contains(Dirty::A11Y)
        );
    }

    #[test]
    fn enough_owed_nodes_become_a_demand_for_the_whole_tree() {
        let mut pending = Pending::new();
        let mut document = Document::new();
        for _ in 0..5000 {
            let node = document.append(
                document.document_index(),
                NodeKind::Element,
                ElementName::new("box"),
            );
            pending.mark(document.store().key_of(node));
        }
        assert!(pending.is_everything());
        assert_eq!(pending.nodes().count(), 0);
    }
}
