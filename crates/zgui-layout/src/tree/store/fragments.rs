//! The pieces boxes were painted as: naming them, reusing them, and destroying them.

use zgui_dom::NodeKey;
use zgui_dom::side::BoxKey;

use crate::fragment::{FragKey, Fragment, FragmentFlags, FragmentKind, ParagraphId};
use crate::tree::store::LayoutStore;

impl LayoutStore {
    /// The fragments one element's boxes produced.
    pub fn fragments_of(&self, node: NodeKey) -> &[FragKey] {
        self.fragments_of_node
            .get(node)
            .map_or(&[], |list| &list[..])
    }

    /// Every fragment that reads pixels outside every rectangle it writes.
    pub fn read_extents(&self) -> &[FragKey] {
        &self.read_extents
    }

    /// One fragment.
    pub fn fragment(&self, key: FragKey) -> Option<&Fragment> {
        self.fragments.get(key)
    }

    /// How many fragments are live.
    pub fn fragment_count(&self) -> u32 {
        self.fragments.len()
    }

    /// The fragments one box produced, in the order it produced them.
    pub fn fragments_of_box(&self, key: BoxKey) -> &[FragKey] {
        self.layout
            .get(key)
            .map_or(&[], |state| &state.fragments[..])
    }

    /// The identifier one shaped paragraph is named by, issuing one if it has none.
    pub fn intern_paragraph(&mut self, key: zgui_text::ParagraphKey) -> ParagraphId {
        if let Some(index) = self.paragraphs.iter().position(|held| *held == key) {
            return ParagraphId(index as u32);
        }
        self.paragraphs.push(key);
        ParagraphId((self.paragraphs.len() - 1) as u32)
    }

    /// The shaped paragraph one identifier names, if any fragment draws it.
    ///
    /// The way back from the name a fragment carries to the key a text engine holds its glyphs
    /// under. A fragment names a paragraph by an index into this store so that comparing two
    /// fragments costs a word; whoever draws the glyphs needs the engine's own key, and this is
    /// the single place the two are related.
    pub fn paragraph_key(&self, id: ParagraphId) -> Option<zgui_text::ParagraphKey> {
        self.paragraphs.get(id.index() as usize).copied()
    }

    /// One fragment, for modification.
    ///
    /// Fragments are updated in place rather than replaced, because their names are what the
    /// hit-test index, the paint cache and the damage of the previous frame all refer to: a
    /// fragment that kept its geometry but changed its name would cost a whole-index rebuild and
    /// throw away a cache entry that was still valid.
    pub(crate) fn fragment_mut(&mut self, key: FragKey) -> Option<&mut Fragment> {
        self.fragments.get_mut(key)
    }

    /// The fragment one box produced in position `slot`, if it produced that many and that one is
    /// the same piece of the box.
    ///
    /// A box that produced three lines and now produces three lines reuses all three names. One
    /// that produced three and now produces four reuses three of them, and one whose pieces changed
    /// shape entirely reuses none — which is decided per position rather than for the whole list,
    /// so a paragraph gaining a line at the end keeps every earlier line's identity.
    ///
    /// The test is [`FragmentKind::same_piece`] and not equality, so a line whose characters
    /// changed keeps its name and owes a repaint rather than ceasing to exist.
    pub(crate) fn reusable_fragment(
        &self,
        box_: BoxKey,
        slot: usize,
        kind: FragmentKind,
    ) -> Option<FragKey> {
        let key = *self.layout.get(box_)?.fragments.get(slot)?;
        self.fragments
            .get(key)?
            .kind
            .same_piece(kind)
            .then_some(key)
    }

    /// Adds one fragment to a box, built by `fill` from an empty one.
    ///
    /// Deliberately not public: the fragment tree has exactly one writer, and it is the pass that
    /// composes absolute geometry. A second producer would be a second opinion about where a piece
    /// of a box is.
    pub(crate) fn insert_fragment(
        &mut self,
        box_: BoxKey,
        fill: impl FnOnce(&mut Fragment),
    ) -> FragKey {
        let node = self.get(box_).and_then(|record| record.source);
        let key = self.fragments.insert_with(|key| {
            let mut fragment = Fragment::new(key, box_, FragmentKind::Box);
            fragment.node = node;
            fill(&mut fragment);
            fragment
        });
        if let Some(state) = self.layout.get_mut(box_) {
            state.fragments.push(key);
        }
        if let Some(node) = node {
            self.fragments_of_node.get_mut(node).push(key);
        }
        key
    }

    /// Removes every fragment one box produced.
    pub(crate) fn clear_fragments(&mut self, box_: BoxKey) {
        self.truncate_fragments(box_, 0);
    }

    /// Removes every fragment one box produced beyond the first `len` of them.
    ///
    /// This is what a rebuild that produced fewer pieces than last time calls: the survivors keep
    /// their names, and the ones that ceased to exist are unregistered everywhere that named them.
    pub(crate) fn truncate_fragments(&mut self, box_: BoxKey, len: usize) {
        let Some(state) = self.layout.get_mut(box_) else {
            return;
        };
        if state.fragments.len() <= len {
            return;
        }
        let dropped: Vec<FragKey> = state.fragments.drain(len..).collect();
        let node = self.get(box_).and_then(|record| record.source);
        for key in dropped {
            self.forget_read_extent(key);
            self.retired.push(key);
            self.fragments.remove(key);
            if let Some(node) = node {
                self.fragments_of_node
                    .get_mut(node)
                    .retain(|&mut it| it != key);
            }
        }
    }

    /// Records whether one fragment reads pixels outside every rectangle it writes.
    ///
    /// Maintained as fragments gain and lose such an extent rather than rebuilt each frame,
    /// because the walk that decides it descends only what changed: a blurred dialog that nothing
    /// touched this frame is never visited, and must still be findable when the content beneath it
    /// animates. Membership is mirrored in the fragment's own flags, so the test that decides
    /// whether a probe is needed at all costs one bit.
    pub(crate) fn set_read_extent(&mut self, key: FragKey, reads_outside: bool) {
        let Some(fragment) = self.fragments.get_mut(key) else {
            return;
        };
        let listed = fragment.flags.contains(FragmentFlags::HAS_READ_EXTENT);
        if listed == reads_outside {
            return;
        }
        if reads_outside {
            fragment.flags = fragment.flags.union(FragmentFlags::HAS_READ_EXTENT);
            self.read_extents.push(key);
        } else {
            fragment.flags = fragment.flags.without(FragmentFlags::HAS_READ_EXTENT);
            if let Some(at) = self.read_extents.iter().position(|held| *held == key) {
                self.read_extents.swap_remove(at);
            }
        }
    }

    /// Takes the fragments destroyed since the last call, leaving the list empty.
    ///
    /// Drained rather than read so that each destroyed name is handed out exactly once: a second
    /// reader would unregister a name a third party has since reused.
    pub(crate) fn drain_retired(&mut self) -> Vec<FragKey> {
        core::mem::take(&mut self.retired)
    }

    /// Takes a fragment that is about to cease to exist out of the read-extent registry.
    fn forget_read_extent(&mut self, key: FragKey) {
        if let Some(at) = self.read_extents.iter().position(|held| *held == key) {
            self.read_extents.swap_remove(at);
        }
    }
}
