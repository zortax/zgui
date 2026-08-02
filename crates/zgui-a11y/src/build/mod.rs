//! Turning a frame into the update an assistive technology is sent.
//!
//! # The three shapes an update takes
//!
//! * **A difference**, built from the nodes the document marked and no others. This is the frame's
//!   ordinary output and it is what makes accessibility cost something proportional to what
//!   changed.
//! * **A whole tree**, built when a consumer has just connected and is holding nothing. There is no
//!   difference to send against nothing.
//! * **Focus alone**, carrying no nodes at all. Focus is part of every update, so a frame that
//!   moved focus and changed nothing else still has something to say.
//!
//! # Two rules that are not optional
//!
//! **Focus is on every update.** accesskit requires the current focus with every one, so it is a
//! field of the value this builds rather than something a caller can forget.
//!
//! **A node's parent is re-projected whenever the node is.** A child list belongs to the parent and
//! to nothing else, so a node that appeared, vanished or changed identity is only visible to a
//! consumer once its parent is sent again. The parent is usually unchanged and is therefore usually
//! not in the update — the cost is one projection, not one node.

pub mod pending;
pub mod snapshot;

/// The tree, the update and the identity a consumer is addressed in.
///
/// Re-exported because a frame loop routes these and does not otherwise name the accessibility
/// interchange vocabulary: they would otherwise need a second name to cross that edge.
pub use accesskit::{Tree as A11yTree, TreeId, TreeUpdate};

use std::collections::BTreeSet;

use accesskit::{Node, NodeId, Tree};
use zgui_dom::{Document, NodeKey};

use crate::build::pending::Pending;
use crate::build::snapshot::Snapshots;
use crate::id::to_a11y;
use crate::project;
use crate::world::World;

/// Builds the updates one window's accessibility tree is advanced by.
///
/// ```
/// use zgui_a11y::A11yBuilder;
///
/// let builder = A11yBuilder::new();
/// assert!(!builder.has_published());
/// ```
#[derive(Default)]
pub struct A11yBuilder {
    /// What the consumer is holding.
    held: Snapshots,
    /// What is owed since the last update was built.
    owed: Pending,
    /// Whether an update has ever been produced.
    published: bool,
}

impl A11yBuilder {
    /// A builder that has sent nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether an update has ever been produced.
    ///
    /// Until one has, the only valid update is a whole tree: a difference is a difference against
    /// something, and a consumer holding nothing has nothing for one to apply to.
    pub fn has_published(&self) -> bool {
        self.published
    }

    /// Whether anything is owed an update.
    pub fn is_owed(&self) -> bool {
        !self.published || self.owed.is_owed()
    }

    /// How many nodes the consumer is holding.
    pub fn held(&self) -> usize {
        self.held.len()
    }

    /// Drains `document`'s accessibility marks, retiring the phase.
    ///
    /// Called on every frame, whether or not anything is listening. See
    /// [`pending`](mod@crate::build::pending) for why that is not wasted work.
    pub fn collect(&mut self, document: &mut Document) -> usize {
        self.owed.collect(document)
    }

    /// Records that `node`'s boxes were carried to a new position and nothing else about it
    /// changed.
    ///
    /// Reported by name rather than through the document's marks, because the marks cannot say
    /// which of the two it is: see
    /// [`DocumentMarks::recording_moves`](zgui_layout::fragment::diff::DocumentMarks::recording_moves).
    ///
    /// This and [`A11yBuilder::collect`] accumulate into the same set of obligations, and a
    /// projection subsumes a move, so a node that owes both ends up owing the projection whichever
    /// order the two arrive in and however many frames apart.
    pub fn note_move(&mut self, node: NodeKey) {
        self.owed.mark_moved(node);
    }

    /// Records that a coordinate system resolves to a different matrix than it did.
    ///
    /// The other half of [`A11yBuilder::note_move`], and the half that does not depend on anything
    /// having walked the fragments. A node whose bounds were measured through `space` is drawn
    /// somewhere else now, whether or not its own rectangle was touched: an element being animated
    /// keeps every rectangle it has and moves because the space under it was written to.
    ///
    /// Costs nothing at all until an update has been published, because what it looks in is what
    /// the consumer is holding — and on a machine with nothing listening, that is empty.
    pub fn note_space_moved(&mut self, space: zgui_scene::SpatialId) {
        let moved: Vec<NodeKey> = self.held.measured_in(space).collect();
        for node in moved {
            self.owed.mark_moved(node);
        }
    }

    /// Forgets everything the consumer was holding, so the next update is a whole tree.
    pub fn forget(&mut self) {
        self.held.clear();
        self.published = false;
        self.owed = Pending::new();
    }

    /// The update this frame owes.
    ///
    /// A whole tree until one has been sent, and a difference afterwards.
    pub fn build(&mut self, world: &World<'_>) -> TreeUpdate {
        if !self.published || self.owed.is_everything() {
            return self.build_full(world);
        }
        let owed = self.owed.take();
        let mut targets: BTreeSet<NodeKey> = BTreeSet::new();
        for node in owed.nodes() {
            targets.insert(node);
            if let Some(parent) = project::children::parent_of(world, node) {
                targets.insert(parent);
            }
        }

        // A node that only moved owes a rectangle, and its parent owes nothing: a child list is
        // what makes a parent's projection stale, and a subtree that was carried somewhere else
        // still holds exactly the children it held. One the consumer is not holding is a different
        // matter — there is no rectangle to replace — so it takes the ordinary path, parent and all.
        let mut remeasured: Vec<NodeKey> = Vec::new();
        for node in owed.moved() {
            if self.held.holds(node) {
                remeasured.push(node);
                continue;
            }
            targets.insert(node);
            if let Some(parent) = project::children::parent_of(world, node) {
                targets.insert(parent);
            }
        }

        // Departures first, projections second, and never interleaved. A node that leaves the tree
        // invalidates every node naming it, and those nodes are usually not in this frame's marks
        // at all — so the whole set of departures has to be known before anything is projected, or
        // a node projected early keeps a relation into a subtree dropped later in the same pass.
        self.retire_departures(world, &mut targets);

        let mut nodes = Vec::new();
        for key in &targets {
            self.reproject(world, *key, &mut nodes);
        }
        // After the projections, and skipping anything they already covered: a node the departure
        // pass pulled in has been projected whole, which answers where it is as well.
        for key in remeasured {
            if targets.contains(&key) {
                continue;
            }
            self.remeasure(world, key, &mut nodes);
        }
        TreeUpdate {
            nodes,
            tree: None,
            tree_id: TreeId::ROOT,
            focus: self.focus_id(world),
        }
    }

    /// A whole tree, for a consumer that is holding nothing.
    pub fn build_full(&mut self, world: &World<'_>) -> TreeUpdate {
        self.held.clear();
        self.owed = Pending::new();
        let root = world.root();
        let mut nodes = Vec::new();
        self.project_subtree(world, root, &mut nodes);
        self.published = true;
        TreeUpdate {
            nodes,
            tree: Some(Tree::new(to_a11y(root))),
            tree_id: TreeId::ROOT,
            focus: self.focus_id(world),
        }
    }

    /// An update that says only where focus is.
    ///
    /// Routed on its own so that focus moving inside a batch of document changes is never lost
    /// behind them.
    pub fn focus_update(&self, world: &World<'_>) -> TreeUpdate {
        TreeUpdate {
            nodes: Vec::new(),
            tree: None,
            tree_id: TreeId::ROOT,
            focus: self.focus_id(world),
        }
    }

    /// Every identifier the consumer is holding.
    pub fn retained(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.held.ids()
    }

    /// Where focus is, as the tree names it.
    ///
    /// The root when nothing holds focus, and also when whatever holds it is not in the tree: an
    /// identifier a consumer cannot resolve is not a focus report but a crash in its own thread.
    fn focus_id(&self, world: &World<'_>) -> NodeId {
        match world.focus.filter(|node| world.is_projected(*node)) {
            Some(node) => to_a11y(node),
            None => to_a11y(world.root()),
        }
    }

    /// Drops everything that has left the tree, and adds whatever named it to `targets`.
    ///
    /// Structure only: which nodes a node still has is answered from the document directly, so
    /// nothing is projected here and the answer does not depend on the order the marks arrived in.
    /// A node that merely moved is left alone — it is still in the tree, and the consumer follows
    /// it to its new parent from that parent's own child list.
    fn retire_departures(&mut self, world: &World<'_>, targets: &mut BTreeSet<NodeKey>) {
        let mut queue: Vec<NodeKey> = targets.iter().copied().collect();
        let mut examined: BTreeSet<NodeKey> = BTreeSet::new();
        while let Some(key) = queue.pop() {
            if !examined.insert(key) {
                continue;
            }
            let departed: Vec<NodeKey> = if world.is_projected(key) {
                let still_here = project::children::of(world, key);
                self.held
                    .children_of(key)
                    .iter()
                    .filter(|id| !still_here.contains(id))
                    .filter_map(|id| crate::id::to_document(*id))
                    .filter(|child| !world.is_projected(*child))
                    .collect()
            } else {
                vec![key]
            };
            for child in departed {
                self.retire_subtree(world, child, targets, &mut queue);
            }
        }
    }

    /// Forgets `key` and everything held below it, queueing every node that named any of them.
    fn retire_subtree(
        &mut self,
        world: &World<'_>,
        key: NodeKey,
        targets: &mut BTreeSet<NodeKey>,
        queue: &mut Vec<NodeKey>,
    ) {
        let mut stack = vec![key];
        while let Some(key) = stack.pop() {
            if world.is_projected(key) {
                // Reparented rather than removed: the consumer keeps it and reaches it through
                // whichever parent now lists it.
                continue;
            }
            let below: Vec<NodeKey> = self
                .held
                .children_of(key)
                .iter()
                .filter_map(|id| crate::id::to_document(*id))
                .collect();
            stack.extend(below);
            self.held.forget(key);
            for referrer in self.held.referrers_of(to_a11y(key)) {
                targets.insert(referrer);
                queue.push(referrer);
            }
        }
    }

    /// Answers a node that was carried somewhere else: its rectangle, and nothing else.
    ///
    /// The claim this rests on is stated where a move is recorded — see
    /// [`FrameDirty::moved`](zgui_layout::fragment::diff::FrameDirty::moved) — and it is that the
    /// subtree owed no work at all, so everything a projection would derive from the document is
    /// what the consumer is already holding. What is left is the rectangle, and it is *measured*
    /// rather than translated, so the value sent is the value a full projection would have
    /// computed rather than an accumulation of offsets.
    fn remeasure(&mut self, world: &World<'_>, key: NodeKey, nodes: &mut Vec<(NodeId, Node)>) {
        if !world.is_projected(key) {
            // Gone since the move was recorded, and already retired along with everything below it.
            return;
        }
        let bounds = project::geometry::bounds_of(world, key);
        let sent = self.held.remeasure(key, bounds);
        self.file_by_space(world, key);
        if let Some(node) = sent {
            nodes.push((to_a11y(key), node));
        }
    }

    /// Files a held node under the coordinate systems its rectangle was measured through.
    ///
    /// Done wherever a rectangle is recorded rather than once at the end, because the two have to
    /// agree: a node filed under a coordinate system it is no longer measured in is answered for a
    /// matrix its bounds do not depend on, and one not filed at all stops being corrected when the
    /// space it is in moves.
    fn file_by_space(&mut self, world: &World<'_>, key: NodeKey) {
        if !world.declares_semantics(key) {
            self.held.measured_through(key, core::iter::empty());
            return;
        }
        let spaces: Vec<_> = project::geometry::spaces_of(world, key).collect();
        self.held.measured_through(key, spaces.into_iter());
    }

    /// Projects one node, putting it in `nodes` only if the consumer would see a difference.
    fn reproject(&mut self, world: &World<'_>, key: NodeKey, nodes: &mut Vec<(NodeId, Node)>) {
        let Some(projected) = project::node(world, key) else {
            // Gone, and already retired along with everything below it.
            return;
        };
        let changed = self.held.record(key, &projected);
        self.file_by_space(world, key);
        if changed {
            nodes.push((to_a11y(key), projected));
        }
    }

    /// Projects `key` and everything below it, in document order.
    ///
    /// Walked over an explicit stack rather than by recursion, because a document's depth is
    /// whatever an application nested and a stack overflow is not a failure mode a tree walk may
    /// have.
    fn project_subtree(
        &mut self,
        world: &World<'_>,
        key: NodeKey,
        nodes: &mut Vec<(NodeId, Node)>,
    ) {
        let mut stack = vec![key];
        while let Some(key) = stack.pop() {
            let Some(projected) = project::node(world, key) else {
                continue;
            };
            let children = projected.children().to_vec();
            self.held.record(key, &projected);
            self.file_by_space(world, key);
            nodes.push((to_a11y(key), projected));
            // Reversed, so that popping visits the children in document order.
            for child in children.into_iter().rev() {
                if let Some(child) = crate::id::to_document(child) {
                    stack.push(child);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
