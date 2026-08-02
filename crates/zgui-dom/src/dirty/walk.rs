//! Visiting the nodes that owe one kind of work, and retiring it as the walk unwinds.
//!
//! Every stage of a frame has the same shape: start at the document node, descend only where
//! something says there is work, do the work on the way down, and forget the obligation on the way
//! back up. Two mechanisms make that cost `O(dirty path)` rather than `O(document)`, and they are
//! independent. The subtree half of a node's invalidation word skips a whole clean subtree in one
//! atomic load. The node's dirty-child record skips a clean *sibling range*, which the union cannot
//! do: a parent with ten thousand children and one dirty child still probes ten thousand words
//! without it, and no counter except [`Counter::DirtyWalkSteps`] can see that happening.
//!
//! # Three rules that look like details and are not
//!
//! **The callback runs on the way down.** Every stage this drives produces output its descendants
//! read, so a post-order visit would hand each child a parent that has not been computed yet.
//!
//! **The bits are retired on the way in, immediately before the callback runs.** A node that
//! re-marks *itself* from inside its callback would otherwise have that mark erased on the way back
//! up, and the surviving union this returns would read empty — the obligation still on the node,
//! and nothing leading to it. Clearing on the way up cannot avoid that: the re-mark sets the very
//! bits the unwind is about to clear, and no bit set distinguishes an obligation that survived from
//! one that was added again. Clearing them the instant before the callback runs does, keeps the
//! pre-order visit, and clears exactly what the node owed and nothing else.
//!
//! **The dirty-child record is rebuilt over every bit, not over the phase being retired.** One
//! record serves every stage. Rebuilding it from "which children still owe *this* phase" would drop
//! a child that owes only accessibility work, and the stage that services that would then never
//! descend to it.

use smallvec::SmallVec;
use zgui_bits::Dirty;
use zgui_profile::{Counter, counter};

use crate::arena::store::DocumentStore;
use crate::id::node_key::NodeIndex;

/// How deep and how wide a walk goes before it touches the heap.
const INLINE: usize = 64;

/// Visits every node at or below `root` that owes `phase`, and retires `phase` as it unwinds.
///
/// `visit` is called for a node **before** its children are visited, and only for nodes whose own
/// obligations include at least one bit of `phase`. Nodes that merely have work below them are
/// descended through without being visited.
///
/// Returns what still survives for `phase` at or below `root`: empty unless a visited node marked
/// itself, or a descendant the walk had not yet reached, from inside `visit`. On return, `root`'s
/// subtree union carries no bit of `phase` that nothing below it owes.
///
/// A node's own obligations for `phase` are cleared the instant before `visit` is called for it, so
/// a callback that marks its own node is asking for the *next* pass rather than undoing this one.
///
/// # Contract on `visit`
///
/// A callback may mark **the node it was called for**, or a descendant of that node this walk has
/// not yet reached. Marking anything else — a sibling, an ancestor, an already-visited descendant —
/// leaves that node's own obligations set with nothing leading back to it, so it is not serviced
/// until something marks it again. A stage that genuinely needs to widen its own reach runs itself
/// a second time instead.
///
/// # Panics
///
/// Panics if `root` names no live node of `store`.
pub fn walk(
    store: &mut DocumentStore,
    root: NodeIndex,
    phase: Dirty,
    visit: &mut impl FnMut(&DocumentStore, NodeIndex),
) -> Dirty {
    let mut scratch = Scratch::new();
    walk_in(&mut scratch, store, root, phase, visit)
}

/// The same walk over buffers the caller keeps, so a frame's several walks allocate nothing.
///
/// # Panics
///
/// Panics if `root` names no live node of `store`.
pub fn walk_in(
    scratch: &mut Scratch,
    store: &mut DocumentStore,
    root: NodeIndex,
    phase: Dirty,
    visit: &mut impl FnMut(&DocumentStore, NodeIndex),
) -> Dirty {
    let store: &DocumentStore = store;
    let (own, subtree) = store.core(root).dirty().get();
    if !(own | subtree).intersects(phase) {
        return Dirty::empty();
    }

    scratch.candidates.clear();
    scratch.stack.clear();
    let entered = enter(store, root, phase, &mut scratch.candidates, visit);
    scratch.stack.push(entered);

    let mut surviving = Dirty::empty();
    while let Some(frame) = scratch.stack.last_mut() {
        if frame.cursor < scratch.candidates.len() {
            let child = scratch.candidates[frame.cursor];
            frame.cursor += 1;
            counter::bump(Counter::DirtyWalkSteps);
            let (own, subtree) = store.core(child).dirty().get();
            if (own | subtree).intersects(phase) {
                let entered = enter(store, child, phase, &mut scratch.candidates, visit);
                scratch.stack.push(entered);
            }
            continue;
        }
        let frame = scratch
            .stack
            .pop()
            .expect("the loop body only runs with a frame on the stack");
        let leaving = leave(store, &frame, phase, &mut scratch.candidates);
        match scratch.stack.last_mut() {
            Some(parent) => parent.surviving |= leaving,
            None => surviving = leaving,
        }
    }
    surviving
}

/// Enters a node: counts it, visits it if it owes the phase, and lists the children to descend to.
fn enter(
    store: &DocumentStore,
    node: NodeIndex,
    phase: Dirty,
    candidates: &mut SmallVec<[NodeIndex; INLINE]>,
    visit: &mut impl FnMut(&DocumentStore, NodeIndex),
) -> Frame {
    counter::bump(Counter::NodesVisited);
    let record = store.core(node);
    let owed = record.dirty().own() & phase;
    if !owed.is_clean() {
        // Retired before the callback rather than after it, so that a callback marking its own node
        // leaves an obligation this walk reports rather than one it erases.
        record.dirty().clear_own(owed);
        visit(store, node);
    }
    // Listed after the callback, so a callback that marks a descendant it has not yet reached is
    // serviced by this walk rather than by the next one.
    let start = candidates.len();
    candidates.extend(store.core(node).dirty_children().iter(store, node));
    Frame {
        node,
        start,
        cursor: start,
        surviving: Dirty::empty(),
    }
}

/// Leaves a node: retires exactly what it owed on entry and rewrites its dirty-child record.
fn leave(
    store: &DocumentStore,
    frame: &Frame,
    phase: Dirty,
    candidates: &mut SmallVec<[NodeIndex; INLINE]>,
) -> Dirty {
    let record = store.core(frame.node);
    // Whatever the node owes for the phase now is a re-mark taken during its own callback: it was
    // cleared before that callback ran.
    let surviving = frame.surviving | (record.dirty().own() & phase);
    record.dirty().retire_phase(phase, surviving);

    let mut kept = frame.start;
    for index in frame.start..candidates.len() {
        let child = candidates[index];
        let (own, subtree) = store.core(child).dirty().get();
        if !(own | subtree).is_clean() {
            candidates[kept] = child;
            kept += 1;
        }
    }
    record.dirty_children().replace(
        frame.node,
        candidates[frame.start..kept].iter().copied(),
        store,
    );
    candidates.truncate(frame.start);
    surviving
}

/// Buffers a walk borrows instead of allocating.
///
/// Cleared on entry and never shrunk, so a caller that keeps one across frames pays for the deepest
/// and widest walk it has ever run and nothing more.
#[derive(Default)]
pub struct Scratch {
    /// Every frame's candidate children, laid end to end.
    candidates: SmallVec<[NodeIndex; INLINE]>,
    /// The nodes the walk is currently inside.
    stack: SmallVec<[Frame; INLINE]>,
}

impl Scratch {
    /// Empty buffers.
    pub fn new() -> Self {
        Self::default()
    }
}

/// One node's state while the walk is inside it.
struct Frame {
    /// The node.
    node: NodeIndex,
    /// Where its candidate children start in the shared buffer.
    start: usize,
    /// How far through those candidates the walk has got.
    cursor: usize,
    /// What its children left outstanding for the phase.
    surviving: Dirty,
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_interned::ElementName;

    use super::walk;
    use crate::arena::document::Document;
    use crate::dirty::propagate::mark;
    use crate::id::node_key::NodeIndex;
    use crate::node::kind::NodeKind;

    /// A chain of `depth` elements under the document node, innermost last.
    fn chain(depth: usize) -> (Document, Vec<NodeIndex>) {
        let mut document = Document::new();
        let mut parent = document.document_index();
        let mut nodes = Vec::new();
        for _ in 0..depth {
            parent = document.append(parent, NodeKind::Element, ElementName::new("box"));
            nodes.push(parent);
        }
        (document, nodes)
    }

    #[test]
    fn a_walk_visits_only_the_nodes_that_owe_the_phase() {
        let (mut document, nodes) = chain(4);
        let leaf = *nodes.last().expect("the chain has nodes");
        mark(document.store_mut(), leaf, Dirty::RESTYLE);

        let mut visited = Vec::new();
        let root = document.document_index();
        let surviving = walk(
            document.store_mut(),
            root,
            Dirty::RESTYLE,
            &mut |_, node| visited.push(node),
        );
        assert_eq!(visited, vec![leaf]);
        assert!(surviving.is_clean());
        assert!(
            !document
                .store()
                .core(root)
                .dirty()
                .subtree()
                .contains(Dirty::RESTYLE),
            "the walk retires the union it drained"
        );
    }

    #[test]
    fn a_second_walk_over_a_drained_tree_visits_nothing() {
        let (mut document, nodes) = chain(4);
        mark(
            document.store_mut(),
            *nodes.last().expect("the chain has nodes"),
            Dirty::RESTYLE,
        );
        let root = document.document_index();
        walk(document.store_mut(), root, Dirty::RESTYLE, &mut |_, _| {});

        let mut visited = 0;
        walk(document.store_mut(), root, Dirty::RESTYLE, &mut |_, _| {
            visited += 1
        });
        assert_eq!(visited, 0);
    }

    #[test]
    fn retiring_one_phase_leaves_another_phase_pending() {
        let (mut document, nodes) = chain(3);
        let leaf = *nodes.last().expect("the chain has nodes");
        mark(document.store_mut(), leaf, Dirty::RESTYLE | Dirty::A11Y);
        let root = document.document_index();

        walk(document.store_mut(), root, Dirty::RESTYLE, &mut |_, _| {});
        let mut visited = Vec::new();
        walk(document.store_mut(), root, Dirty::A11Y, &mut |_, node| {
            visited.push(node)
        });
        assert_eq!(
            visited,
            vec![leaf],
            "retiring the restyle must not clobber the union the other phase is waiting on"
        );
    }

    #[test]
    fn a_re_mark_of_the_visited_node_survives_its_own_unwind() {
        let (mut document, nodes) = chain(3);
        let leaf = *nodes.last().expect("the chain has nodes");
        mark(document.store_mut(), leaf, Dirty::RESTYLE);
        let root = document.document_index();

        let mut once = false;
        let surviving = walk(
            document.store_mut(),
            root,
            Dirty::RESTYLE,
            &mut |store, node| {
                if !core::mem::replace(&mut once, true) {
                    store.core(node).dirty().mark(Dirty::RESTYLE);
                }
            },
        );
        assert!(
            surviving.contains(Dirty::RESTYLE),
            "clearing the whole phase on the unwind would have swallowed the re-mark"
        );
        assert!(
            document
                .store()
                .core(leaf)
                .dirty()
                .own()
                .contains(Dirty::RESTYLE)
        );
    }

    #[test]
    fn a_child_owing_a_different_phase_stays_in_the_record() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let restyled = document.append(root, NodeKind::Element, ElementName::new("a"));
        let announced = document.append(root, NodeKind::Element, ElementName::new("b"));
        mark(document.store_mut(), restyled, Dirty::RESTYLE);
        mark(document.store_mut(), announced, Dirty::A11Y);

        let document_index = document.document_index();
        walk(
            document.store_mut(),
            document_index,
            Dirty::RESTYLE,
            &mut |_, _| {},
        );

        let kept: Vec<_> = document
            .store()
            .core(root)
            .dirty_children()
            .iter(document.store(), root)
            .collect();
        assert!(
            kept.contains(&announced),
            "a record rebuilt over the retired phase alone would have dropped it, and nothing \
             would ever descend to it again"
        );
        assert!(!kept.contains(&restyled));
    }
}
