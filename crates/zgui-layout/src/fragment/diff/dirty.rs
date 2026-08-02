//! What the fragment pass is allowed to skip, and the two honest answers to it.

use zgui_bits::Dirty;

/// What the fragment pass needs to know about invalidation.
///
/// Stated as a trait rather than as a document borrow because there are two honest answers to
/// "what is dirty" and both are needed. A frame driving an incremental pass answers from the
/// document's own marks. A first build — or a test that wants a fragment tree and has no frame
/// around it — answers "everything", and that answer has to be as real as the other one or the
/// incremental path would be the only one ever exercised.
///
/// The questions are about *elements*, because that is where invalidation is recorded, and a box
/// with no element of its own — an anonymous wrapper, a line of text — has no marks to read. Such a
/// box is never dismissed: it is asked about with `None`, and the answer to that has to be "assume
/// everything", or a wrapper would hide the dirty children below it.
pub trait FrameDirty {
    /// What one element owes itself.
    fn own(&self, node: Option<zgui_dom::NodeKey>) -> Dirty;
    /// What everything below it owes.
    fn subtree(&self, node: Option<zgui_dom::NodeKey>) -> Dirty;
    /// Records that one element owes `bits`.
    fn mark(&mut self, node: Option<zgui_dom::NodeKey>, bits: Dirty);

    /// Whether `node` carries accessibility semantics beyond a plain layout box.
    ///
    /// A widget that moved is a widget whose accessibility node changed, because that node's
    /// bounds are geometry. A *layout box* that moved is not, and the difference is what keeps the
    /// claim honest: a scrolled list of five thousand rows moves five thousand boxes and changes
    /// the accessibility tree only where something declared what it means.
    ///
    /// Answering `false` is always safe for a pass that has no document to ask.
    fn is_semantic(&self, node: Option<zgui_dom::NodeKey>) -> bool {
        let _ = node;
        false
    }

    /// Records that `node`'s boxes were carried to a new position and nothing else about it
    /// changed.
    ///
    /// Stated apart from [`FrameDirty::mark`] because it is a strictly smaller claim, and the
    /// difference is worth something to whoever services it: a node that only moved projects to
    /// what the consumer already holds with one rectangle replaced, so the answer is to measure it
    /// again rather than to derive its role, its name, its relations, its actions and its child
    /// list a second time.
    ///
    /// The default answers it as an ordinary accessibility obligation, which is always correct and
    /// never cheaper.
    fn moved(&mut self, node: Option<zgui_dom::NodeKey>) {
        self.mark(node, Dirty::A11Y);
    }

    /// Forgets `phase` at and below `node`, the pass having just serviced it.
    ///
    /// The pass that reads a phase is the only thing that can retire it, because it is the only
    /// thing that knows the work is done. Left set, an obligation is permanent: every box is asked
    /// about on every frame, no subtree is ever skipped, every fragment is treated as changed and
    /// the damage grows to the root's ink — the whole window, for ever, whatever actually moved.
    fn retire(&mut self, node: Option<zgui_dom::NodeKey>, phase: Dirty);
}

/// What one box owes, and under whose name it is recorded.
///
/// Produced by [`Owed::of`], which is the one place the difference between a box that came from an
/// element and one that did not is turned into an answer.
#[derive(Clone, Copy, Debug)]
pub struct Owed {
    /// The element this box's obligations are read and written under, if there is one.
    pub node: Option<zgui_dom::NodeKey>,
    /// What this box itself owes.
    pub own: Dirty,
    /// What everything below it owes.
    pub subtree: Dirty,
}

impl Owed {
    /// What a box owes, given the element it came from and the one it was generated for.
    ///
    /// A box that came from an element is asked about under its own name, and the two answers are
    /// the document's own.
    ///
    /// A box that came from no element — the anonymous wrapper CSS requires around a run of inline
    /// siblings, the box that establishes an inline formatting context, a run of text between two
    /// child elements — has no marks of its own, and asking about no element at all gets
    /// [`Dirty::all()`]: everything owed, always, by the box *and* its whole subtree. Nearly a fifth
    /// of a real document's boxes are anonymous, and because the subtree answer is consulted before
    /// a clean child is left alone, each of them makes its entire subtree unskippable. That is what
    /// makes the fragment pass proportional to the document on a frame that changed one element.
    ///
    /// So it is asked about under the name of the element it was **generated for**, which is the
    /// nearest box above it that has a style of its own. That element owes everything the wrapper
    /// owes: the wrapper is styled from its style and from nothing else, and everything below the
    /// wrapper came from that same element's content.
    ///
    /// Its **own** answer takes the generator's subtree in as well, and that is not caution. An
    /// anonymous box that establishes an inline formatting context draws the lines its inline
    /// descendants' glyphs sit in — those descendants generate no fragments themselves — so text
    /// that was re-shaped inside it is owed by *this box's own pieces* while being marked on a node
    /// below it. Reading only the generator's own bits leaves a line of changed characters on the
    /// screen exactly as it was, since its rectangle did not move and nothing else would notice.
    pub fn of(
        dirty: &impl FrameDirty,
        source: Option<zgui_dom::NodeKey>,
        generator: Option<zgui_dom::NodeKey>,
    ) -> Self {
        if let Some(node) = source {
            return Self {
                node: source,
                own: dirty.own(Some(node)),
                subtree: dirty.subtree(Some(node)),
            };
        }
        let subtree = dirty.subtree(generator);
        Self {
            node: generator,
            own: dirty.own(generator) | subtree,
            subtree,
        }
    }
}

/// The answer that treats every box as needing everything.
///
/// This is what a first build wants, and it is not a stub: a document being laid out for the first
/// time genuinely owes every fragment, and a pass over it must not skip a subtree because nothing
/// marked it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Everything;

impl FrameDirty for Everything {
    fn own(&self, _node: Option<zgui_dom::NodeKey>) -> Dirty {
        Dirty::all()
    }

    fn subtree(&self, _node: Option<zgui_dom::NodeKey>) -> Dirty {
        Dirty::all()
    }

    fn mark(&mut self, _node: Option<zgui_dom::NodeKey>, _bits: Dirty) {}

    // Nothing to forget: this answer is not read out of a document and writing to one would be
    // inventing state. A first build services every box by construction, so a caller using this
    // has no obligations outstanding for the pass to retire.
    fn retire(&mut self, _node: Option<zgui_dom::NodeKey>, _phase: Dirty) {}
}

/// The answer a frame gives: what the document itself was told.
///
/// Marks written through this reach the ancestors as well as the node, because a mark nothing leads
/// back to is a mark no traversal ever services — the stage that would have serviced it descends
/// from the root and dismisses the subtree on its way past.
pub struct DocumentMarks<'a> {
    /// The document whose marks are read and written.
    store: &'a mut zgui_dom::DocumentStore,
    /// Where the elements that only moved are listed, when a caller asked for them by name.
    moves: Option<&'a mut Vec<zgui_dom::NodeKey>>,
}

impl<'a> DocumentMarks<'a> {
    /// Reads and writes `store`'s marks.
    pub fn new(store: &'a mut zgui_dom::DocumentStore) -> Self {
        Self { store, moves: None }
    }

    /// The same, over a document rather than over its storage.
    ///
    /// A frame holds the document, not its storage, and reaching through to the storage is
    /// deliberately not something every crate may do — the exclusive accessor is the last step
    /// before a write that owes the style engine nothing. This is the one narrow use that is not
    /// such a write: the marks are read, and the marks written are the fragment pass's own.
    pub fn for_document(document: &'a mut zgui_dom::Document) -> Self {
        Self::new(document.store_mut())
    }

    /// Lists every element reported to [`FrameDirty::moved`] into `sink` instead of marking it.
    ///
    /// A move is recorded by name rather than as a bit because the two say different things and the
    /// lattice can only carry one of them. Written as a mark it becomes "this node owes an
    /// accessibility projection", indistinguishable from a node whose label changed, and servicing
    /// it costs a walk to every ancestor to raise the union, a walk back down to drain it, and a
    /// full projection at the end. Listed here it stays what it is — a node that is somewhere else
    /// — and the whole of that is skipped.
    ///
    /// A caller that does not ask keeps the mark, so nothing is lost by not asking.
    pub fn recording_moves(mut self, sink: &'a mut Vec<zgui_dom::NodeKey>) -> Self {
        self.moves = Some(sink);
        self
    }
}

impl FrameDirty for DocumentMarks<'_> {
    fn own(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        match node.and_then(|node| self.store.index_of(node)) {
            Some(index) => self.store.core(index).dirty().own(),
            None => Dirty::all(),
        }
    }

    fn subtree(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        match node.and_then(|node| self.store.index_of(node)) {
            Some(index) => self.store.core(index).dirty().subtree(),
            None => Dirty::all(),
        }
    }

    fn mark(&mut self, node: Option<zgui_dom::NodeKey>, bits: Dirty) {
        let Some(index) = node.and_then(|node| self.store.index_of(node)) else {
            return;
        };
        zgui_dom::dirty::propagate::mark(self.store, index, bits);
    }

    fn moved(&mut self, node: Option<zgui_dom::NodeKey>) {
        let Some(node) = node else {
            return;
        };
        match self.moves.as_mut() {
            Some(sink) => sink.push(node),
            None => self.mark(Some(node), Dirty::A11Y),
        }
    }

    fn is_semantic(&self, node: Option<zgui_dom::NodeKey>) -> bool {
        node.is_some_and(|node| {
            self.store
                .columns()
                .semantics
                .get(node)
                .and_then(|slot| slot.as_deref())
                .is_some_and(|semantics| !semantics.is_trivial())
        })
    }

    fn retire(&mut self, node: Option<zgui_dom::NodeKey>, phase: Dirty) {
        let Some(index) = node.and_then(|node| self.store.index_of(node)) else {
            return;
        };
        zgui_dom::dirty::walk::walk(self.store, index, phase, &mut |_, _| {});
    }
}
