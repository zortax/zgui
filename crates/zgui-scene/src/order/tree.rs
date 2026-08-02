// DERIVED-FROM: the GPUI project, crates/gpui/src/bounds_tree.rs (Apache-2.0)
// The algorithm here — an R-tree whose nodes cache the highest draw order in their subtree, queried
// for "the greatest order among everything this rectangle intersects" with pruning on both the
// cached order and the bounding box, plus the global-maximum fast path, the highest-order-child-last
// invariant, the insert-above-all barrier and the order floor — is adapted from that work, licensed
// under the Apache License, Version 2.0. It has been modified to work over this crate's coordinate
// spaces, to address nodes by index rather than by raw pointer, to report its insertions to the
// frame counters, and to re-enter the previous insert's descent path rather than restart every
// descent at the root.

//! The R-tree that assigns draw order.

use smallvec::SmallVec;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_profile::{Counter, counter};

use crate::id::DrawOrder;

/// How many children an internal node holds before it splits.
///
/// Wide rather than binary: a shallower tree means fewer nodes touched per query, and the per-node
/// work is a bounding-box test that is cheap enough for the width to pay for itself.
const MAX_CHILDREN: usize = 12;

/// What a node is.
#[derive(Clone, Debug)]
enum NodeKind {
    /// One inserted rectangle.
    Leaf,
    /// A group of nodes, kept with the highest-order child last.
    Internal {
        /// Indices into [`BoundsTree::nodes`].
        children: SmallVec<[u32; MAX_CHILDREN]>,
    },
}

/// One node of the tree.
#[derive(Clone, Debug)]
struct Node {
    /// The rectangle containing this node and everything below it.
    bounds: Rect<DevicePx, Device>,
    /// The highest draw order anywhere below this node.
    max_order: DrawOrder,
    /// Whether this is a leaf, and if not, what it holds.
    kind: NodeKind,
}

/// Assigns each inserted rectangle the lowest draw order that still puts it above everything it
/// overlaps.
///
/// Two consequences are what the rest of the crate is built on:
///
/// * **Disjoint content reuses low orders**, so a page of a hundred non-overlapping boxes ends up
///   with a hundred primitives at order one, which a renderer draws in one batch.
/// * **Equal order implies no overlap.** Anything overlapping something already inserted is given a
///   strictly higher order, so two primitives that end up equal cannot be on top of one another —
///   which is why the sequence of primitive kinds at equal order is free to be chosen for batching
///   rather than for correctness.
///
/// Two barriers sit on top of the query. [`BoundsTree::insert_above_all`] ignores overlap entirely
/// and takes one more than the global maximum, which is what a group's marker needs so that its
/// order range cannot be entered by unrelated content reusing a low order.
/// [`BoundsTree::set_order_floor`] raises the minimum for everything inserted afterwards, which is
/// what keeps a later non-overlapping sibling from sorting *inside* a group that has just closed.
///
/// ```
/// use zgui_geom::{Device, DevicePx, Point, Rect, Size};
/// use zgui_scene::BoundsTree;
///
/// let rect = |x: f32| -> Rect<DevicePx, Device> {
///     Rect::new(
///         Point::new(DevicePx(x), DevicePx(0.0)),
///         Size::new(DevicePx(10.0), DevicePx(10.0)),
///     )
/// };
///
/// let mut tree = BoundsTree::new();
/// assert_eq!(tree.insert(rect(0.0)), 1);
/// assert_eq!(tree.insert(rect(5.0)), 2, "overlaps the first, so it sorts above it");
/// assert_eq!(tree.insert(rect(100.0)), 1, "disjoint, so it reuses the low order");
/// ```
#[derive(Clone, Debug, Default)]
pub struct BoundsTree {
    /// Every node, contiguous so a traversal walks memory rather than pointers.
    nodes: Vec<Node>,
    /// The root, if anything has been inserted.
    root: Option<u32>,
    /// The leaf holding the highest order, for the fast path.
    max_leaf: Option<u32>,
    /// The lowest order any subsequent insert may take.
    order_floor: DrawOrder,
    /// Reusable descent path, so an insert allocates nothing.
    path: Vec<u32>,
    /// Reusable search stack, so a query allocates nothing.
    stack: Vec<u32>,
}

impl BoundsTree {
    /// An empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empties the tree and drops its order floor.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.root = None;
        self.max_leaf = None;
        self.order_floor = 0;
        self.path.clear();
        self.stack.clear();
    }

    /// How many rectangles have been inserted.
    pub fn len(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| matches!(node.kind, NodeKind::Leaf))
            .count()
    }

    /// Whether nothing has been inserted.
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// The highest order assigned so far, or zero when nothing has been inserted.
    pub fn max_order(&self) -> DrawOrder {
        self.max_leaf
            .map_or(0, |leaf| self.nodes[leaf as usize].max_order)
    }

    /// The lowest order a subsequent insert may take.
    pub fn order_floor(&self) -> DrawOrder {
        self.order_floor
    }

    /// Raises the minimum order for everything inserted afterwards.
    ///
    /// Relative order above the floor is preserved: overlapping inserts still step above one
    /// another. This is what a closing group and a deferred overlay both need — without it, the
    /// next non-overlapping thing drawn would take a low order and sort *underneath* content that
    /// was painted before it.
    pub fn set_order_floor(&mut self, floor: DrawOrder) {
        self.order_floor = self.order_floor.max(floor);
    }

    /// Inserts `bounds` and returns one more than the highest order it overlaps, never below the
    /// order floor.
    pub fn insert(&mut self, bounds: Rect<DevicePx, Device>) -> DrawOrder {
        let order = (self.max_intersecting(bounds) + 1).max(self.order_floor);
        let leaf = self.insert_leaf(bounds, order);
        self.max_leaf = match self.max_leaf {
            Some(previous) if self.nodes[previous as usize].max_order >= order => Some(previous),
            _ => Some(leaf),
        };
        counter::bump(Counter::BoundsTreeInserts);
        order
    }

    /// Inserts `bounds` above *everything*, whether it overlaps or not.
    ///
    /// A group's markers need this rather than an ordinary insert: an ordinary one would let
    /// unrelated content elsewhere on the surface reuse an order that falls inside the group's
    /// range, and that content would then be swept into the group's target.
    pub fn insert_above_all(&mut self, bounds: Rect<DevicePx, Device>) -> DrawOrder {
        let order = self.max_order() + 1;
        let leaf = self.insert_leaf(bounds, order);
        self.max_leaf = Some(leaf);
        counter::bump(Counter::BoundsTreeInserts);
        order
    }

    /// The highest order among everything `query` intersects, or zero when it intersects nothing.
    fn max_intersecting(&mut self, query: Rect<DevicePx, Device>) -> DrawOrder {
        let Some(root) = self.root else { return 0 };

        // The globally highest leaf answers most queries outright, because interface content is
        // mostly stacked rather than scattered.
        if let Some(leaf) = self.max_leaf {
            let node = &self.nodes[leaf as usize];
            if query.intersects(node.bounds) {
                return node.max_order;
            }
        }

        self.stack.clear();
        self.stack.push(root);
        let mut found = 0;
        while let Some(index) = self.stack.pop() {
            let node = &self.nodes[index as usize];
            // Nothing below this node can beat what has already been found, so do not look.
            if node.max_order <= found || !query.intersects(node.bounds) {
                continue;
            }
            match &node.kind {
                NodeKind::Leaf => found = found.max(node.max_order),
                NodeKind::Internal { children } => {
                    // The highest-order child is last, so pushing in order pops it first and the
                    // pruning above gets its best chance.
                    self.stack.extend(
                        children
                            .iter()
                            .copied()
                            .filter(|child| self.nodes[*child as usize].max_order > found),
                    );
                }
            }
        }
        found
    }

    /// Files a leaf into the tree and returns its index.
    fn insert_leaf(&mut self, bounds: Rect<DevicePx, Device>, order: DrawOrder) -> u32 {
        let leaf = self.push_node(Node {
            bounds,
            max_order: order,
            kind: NodeKind::Leaf,
        });

        let Some(root) = self.root else {
            self.root = Some(leaf);
            return leaf;
        };

        if matches!(self.nodes[root as usize].kind, NodeKind::Leaf) {
            let root_bounds = self.nodes[root as usize].bounds;
            let root_order = self.nodes[root as usize].max_order;
            let children = ordered_pair(root, root_order, leaf, order);
            let new_root = self.push_node(Node {
                bounds: root_bounds.union(bounds),
                max_order: root_order.max(order),
                kind: NodeKind::Internal { children },
            });
            self.root = Some(new_root);
            return leaf;
        }

        self.descend_and_attach(root, leaf, bounds, order);
        self.propagate(bounds, order);
        leaf
    }

    /// Walks down from `from`, attaching `leaf` where it costs the least growth.
    ///
    /// The descent is re-entered part-way down whenever the previous one already reached a node
    /// that contains this rectangle: see [`BoundsTree::resume`].
    fn descend_and_attach(
        &mut self,
        from: u32,
        leaf: u32,
        bounds: Rect<DevicePx, Device>,
        order: DrawOrder,
    ) {
        let mut current = self.resume(bounds).unwrap_or(from);
        loop {
            self.path.push(current);
            let NodeKind::Internal { children } = &self.nodes[current as usize].kind else {
                unreachable!("descent only visits internal nodes");
            };
            // A plain scan rather than a `min_by` over constructed rectangles: this runs once per
            // child per level of every insert, and it is the single hottest loop the scene has.
            // The first of several equal candidates wins, which is what `min_by` does too.
            let mut candidates = children.iter().copied().enumerate();
            let (mut best_position, mut best) =
                candidates.next().expect("an internal node has children");
            let mut best_cost = merged_half_perimeter(bounds, self.nodes[best as usize].bounds);
            for (position, child) in candidates {
                let cost = merged_half_perimeter(bounds, self.nodes[child as usize].bounds);
                if cost < best_cost {
                    best_cost = cost;
                    best = child;
                    best_position = position;
                }
            }

            if matches!(self.nodes[best as usize].kind, NodeKind::Internal { .. }) {
                current = best;
                continue;
            }

            let room = match &self.nodes[current as usize].kind {
                NodeKind::Internal { children } => children.len() < MAX_CHILDREN,
                NodeKind::Leaf => false,
            };
            if room {
                self.attach_child(current, leaf, bounds, order);
            } else {
                self.split_against(current, best, best_position, leaf, bounds, order);
            }
            return;
        }
    }

    /// The deepest node of the previous descent that already contains `bounds`, with the path
    /// truncated to that node's ancestors.
    ///
    /// A document is emitted in painting order, so consecutive rectangles are nearly always
    /// neighbours on the screen: a box's background, its border, its shadow and every glyph of the
    /// line inside it arrive one after another within a few dozen pixels of each other. Starting
    /// each of them at the root re-derives, level by level and child by child, the same answer the
    /// one before it reached.
    ///
    /// **Which node a rectangle is filed under changes nothing about the order it is given.** The
    /// order comes from [`BoundsTree::max_intersecting`], which prunes on two facts — a node's
    /// rectangle contains everything below it, and its cached order is the highest below it — and
    /// returns the greatest order among the leaves the query really meets whatever shape the
    /// hierarchy has. Both facts are maintained by [`BoundsTree::propagate`] over the whole path,
    /// which is why the *ancestors* are kept rather than discarded: they are the nodes that still
    /// have to be told.
    ///
    /// Resuming is only ever offered a node that already contains the rectangle, so nothing here
    /// widens a node that the full descent would have left alone.
    fn resume(&mut self, bounds: Rect<DevicePx, Device>) -> Option<u32> {
        for index in (0..self.path.len()).rev() {
            let node = self.path[index];
            // A path holds interior nodes and never leaves, so every entry of it is somewhere a
            // descent can carry on from.
            if self.nodes[node as usize].bounds.contains_rect(bounds) {
                self.path.truncate(index);
                return Some(node);
            }
        }
        self.path.clear();
        None
    }

    /// Adds `leaf` directly to `parent`, keeping the highest-order child last.
    fn attach_child(
        &mut self,
        parent: u32,
        leaf: u32,
        bounds: Rect<DevicePx, Device>,
        order: DrawOrder,
    ) {
        let node = &mut self.nodes[parent as usize];
        let was_max = node.max_order;
        if let NodeKind::Internal { children } = &mut node.kind {
            children.push(leaf);
            if order <= was_max {
                let last = children.len() - 1;
                children.swap(last - 1, last);
            }
        }
        node.bounds = node.bounds.union(bounds);
        node.max_order = was_max.max(order);
    }

    /// Pairs `leaf` with the full node's best child under a new internal node.
    fn split_against(
        &mut self,
        parent: u32,
        sibling: u32,
        sibling_position: usize,
        leaf: u32,
        bounds: Rect<DevicePx, Device>,
        order: DrawOrder,
    ) {
        let sibling_bounds = self.nodes[sibling as usize].bounds;
        let sibling_order = self.nodes[sibling as usize].max_order;
        let children = ordered_pair(sibling, sibling_order, leaf, order);
        let merged_order = sibling_order.max(order);
        let internal = self.push_node(Node {
            bounds: sibling_bounds.union(bounds),
            max_order: merged_order,
            kind: NodeKind::Internal { children },
        });

        let node = &mut self.nodes[parent as usize];
        let parent_max = node.max_order;
        if let NodeKind::Internal { children } = &mut node.kind {
            let last = children.len() - 1;
            children[sibling_position] = internal;
            if merged_order > parent_max {
                children.swap(sibling_position, last);
            }
        }
        node.bounds = node.bounds.union(bounds);
        node.max_order = parent_max.max(order);
    }

    /// Widens bounds and raises orders back up the descent path.
    fn propagate(&mut self, bounds: Rect<DevicePx, Device>, order: DrawOrder) {
        let mut raised: Option<u32> = None;
        for index in (0..self.path.len()).rev() {
            let node_index = self.path[index];
            let node = &mut self.nodes[node_index as usize];
            node.bounds = node.bounds.union(bounds);
            if node.max_order < order {
                node.max_order = order;
                if let (Some(child), NodeKind::Internal { children }) = (raised, &mut node.kind)
                    && let Some(position) = children.iter().position(|held| *held == child)
                {
                    let last = children.len() - 1;
                    children.swap(position, last);
                }
            }
            raised = Some(node_index);
        }
    }

    /// Appends a node and returns its index.
    fn push_node(&mut self, node: Node) -> u32 {
        self.nodes.push(node);
        (self.nodes.len() - 1) as u32
    }
}

/// Two children with the higher-order one last.
fn ordered_pair(
    first: u32,
    first_order: DrawOrder,
    second: u32,
    second_order: DrawOrder,
) -> SmallVec<[u32; MAX_CHILDREN]> {
    let mut children = SmallVec::new();
    if second_order > first_order {
        children.push(first);
        children.push(second);
    } else {
        children.push(second);
        children.push(first);
    }
    children
}

/// Half a rectangle's perimeter, which is the growth cost a descent minimises.
fn half_perimeter(rect: Rect<DevicePx, Device>) -> f32 {
    rect.size.width.0 + rect.size.height.0
}

/// Half the perimeter of the smallest rectangle containing both.
///
/// Measured without building that rectangle. It is the same arithmetic on the same numbers —
/// the union's width is its right edge less its left edge, and both are the same min and max —
/// so it answers what `half_perimeter(one.union(two))` answers, including for an empty operand,
/// which a union takes to contribute nothing.
fn merged_half_perimeter(one: Rect<DevicePx, Device>, two: Rect<DevicePx, Device>) -> f32 {
    if one.is_empty() {
        return half_perimeter(two);
    }
    if two.is_empty() {
        return half_perimeter(one);
    }
    let left = one.left().0.min(two.left().0);
    let top = one.top().0.min(two.top().0);
    let right = one.right().0.max(two.right().0);
    let bottom = one.bottom().0.max(two.bottom().0);
    (right - left) + (bottom - top)
}
