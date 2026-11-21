//! The paint order mirrored as the compositor's order tree.
//!
//! Each box with children, each box that establishes a stacking context and each box drawn with a
//! group effect gets an order node. A node lists the chunks of its box's fragments, then each child
//! in painting order: a child with a node of its own as that node, any other child as the chunks
//! of its fragments. A child's backdrop chunk comes right before it, outside its node.
//!
//! A box's list is rebuilt in these cases:
//!
//! - The layout store marks the order of its children as dirty.
//! - A child's place in it changed: a fragment came or went, or the child gained or lost its node.
//! - A child sorts by the place of other boxes, such as an absolutely positioned box ranked where
//!   it was written. No mark covers those boxes, so the list is checked after a sync that saw an
//!   order mark below its box.
//!
//! A rebuilt list is written as one splice per run of entries that differs, and a list that did
//! not change writes nothing.

mod effect;
mod splice;

use crate::Tables;
use compositor_types::{OrderEntry, OrderId, OrderNode, OutputId, Transaction};
use rustc_hash::{FxHashMap, FxHashSet};
use zgui_dom::side::BoxKey;
use zgui_layout::{JournalVersion, LayoutStore};
use zgui_space::ChangeCoverage;

use crate::emit::group::GroupEffect;
use crate::sync::key::{self, Part};
use crate::sync::mirror::Mirror;
use crate::walk::stacking;

use self::splice::splice;

/// The order node an output draws, and the order node of every box that has one.
#[derive(Debug, Default)]
pub(crate) struct Order {
    root: Option<OrderId>,
    top: Option<BoxKey>,
    root_entries: Vec<OrderEntry>,
    /// Whether the root's list changed without its box changing, as a backdrop chunk does.
    root_dirty: bool,
    nodes: FxHashMap<BoxKey, Node>,
    placed: FxHashMap<BoxKey, Place>,
    effects: FxHashMap<BoxKey, GroupEffect>,
    version: Option<JournalVersion>,
    dirty: FxHashSet<BoxKey>,
    restyled: FxHashSet<BoxKey>,
    /// Entries spliced into the lists of nodes created during the current sync.
    created: usize,
    /// The boxes whose list holds a child that sorts by other boxes than itself, which no mark
    /// covers, so a sync after an order mark below one of them checks the order of its children.
    volatile: FxHashSet<BoxKey>,
    scratch: Scratch,
}

/// The order node of one box.
#[derive(Debug)]
struct Node {
    id: OrderId,
    entries: Vec<OrderEntry>,
    children: Vec<BoxKey>,
    effect: Option<effect::Written>,
}

/// The list a box is placed in.
#[derive(Clone, Copy, Debug)]
struct Place {
    /// The box whose node lists it.
    owner: BoxKey,
    /// Whether it is listed as its own node.
    node: bool,
}

/// What one sync of the order tree wrote.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Synced {
    /// Entries removed and inserted by splices.
    pub(crate) entries: usize,
    /// Effects written onto order nodes.
    pub(crate) effects: usize,
}

/// Containers one sync reuses.
#[derive(Debug, Default)]
struct Scratch {
    marked: Vec<BoxKey>,
    orphans: Vec<BoxKey>,
    visited: FxHashSet<BoxKey>,
    volatile: Vec<BoxKey>,
    seen: FxHashSet<BoxKey>,
    entries: Vec<OrderEntry>,
}

/// What a sync of the order tree reads and writes besides the order tree.
pub(crate) struct Context<'a> {
    pub(crate) tx: &'a mut Transaction,
    pub(crate) store: &'a LayoutStore,
    pub(crate) mirror: &'a mut Mirror,
    pub(crate) tables: &'a mut Tables,
    /// Whether a box has an outline chunk.
    pub(crate) outlined: &'a dyn Fn(BoxKey) -> bool,
    /// Whether a box has a backdrop chunk.
    pub(crate) backdropped: &'a dyn Fn(BoxKey) -> bool,
}

impl Order {
    /// Creates the root node and makes `output` draw it, unless that is done.
    pub(crate) fn ensure_root(&mut self, tx: &mut Transaction, output: OutputId) {
        if self.root.is_none() {
            let root = tx.create_order(OrderNode::default());
            tx.set_root(output, Some(root));
            self.root = Some(root);
        }
    }

    /// Notes that the fragments of the box `key` came or went.
    pub(crate) fn fragments_changed(&mut self, key: BoxKey) {
        self.dirty.insert(key);
    }

    /// Notes that the box `key` gained or lost its chunk of `part`.
    ///
    /// The box's own list holds its outline. The list that holds the box holds its backdrop.
    pub(crate) fn part_changed(&mut self, key: BoxKey, part: Part) {
        self.dirty.insert(key);
        if part != Part::Backdrop {
            return;
        }
        match self.placed.get(&key) {
            Some(place) => {
                self.dirty.insert(place.owner);
            }
            None => self.root_dirty |= self.top == Some(key),
        }
    }

    /// Notes the group effect the box `key` is drawn with, `None` for none.
    pub(crate) fn isolate(&mut self, key: BoxKey, effect: Option<GroupEffect>) {
        let changed = match effect {
            Some(effect) => self.effects.insert(key, effect.clone()).as_ref() != Some(&effect),
            None => self.effects.remove(&key).is_some(),
        };
        if changed {
            self.restyled.insert(key);
            self.dirty.insert(key);
        }
    }

    /// Brings the order tree up to the painting order of `store` and the effects noted, and
    /// removes the nodes no list names any more.
    pub(crate) fn sync(&mut self, context: &mut Context<'_>) -> Synced {
        let Some(root) = self.root else {
            return Synced::default();
        };
        let mut work = std::mem::take(&mut self.scratch.marked);
        work.clear();
        let coverage = match self.version {
            Some(version) => context.store.order_dirty_since(version, &mut work),
            None => ChangeCoverage::All,
        };
        self.version = Some(context.store.order_version());
        if coverage == ChangeCoverage::All {
            work.extend(self.nodes.keys().copied());
        }
        let mut volatile = std::mem::take(&mut self.scratch.volatile);
        self.volatile_above(context.store, &work, coverage, &mut volatile);
        work.extend(self.dirty.drain());

        self.scratch.visited.clear();
        let mut entries = self.sync_root(context, root);
        entries += self.drain(context, &mut work);
        for &key in &volatile {
            if self.scratch.visited.contains(&key) {
                continue;
            }
            if self.keeps_children(context.store, key) {
                self.scratch.visited.insert(key);
                continue;
            }
            work.push(key);
            entries += self.drain(context, &mut work);
        }
        entries += std::mem::take(&mut self.created);
        let effects = self.write_effects(context);
        self.remove_orphans(context);
        self.scratch.marked = work;
        self.scratch.volatile = volatile;
        Synced { entries, effects }
    }

    /// Lists the layout's root box in the output's root node, and returns the entries spliced.
    fn sync_root(&mut self, context: &mut Context<'_>, root: OrderId) -> usize {
        let top = context
            .store
            .root()
            .filter(|top| context.store.get(*top).is_some());
        let dirty = core::mem::take(&mut self.root_dirty);
        if top == self.top && top.is_none_or(|top| self.nodes.contains_key(&top)) && !dirty {
            return 0;
        }
        if let Some(old) = self.top.take()
            && Some(old) != top
        {
            self.scratch.orphans.push(old);
        }
        self.top = top;
        let mut entries = Vec::new();
        if let Some(top) = top {
            push_backdrop(context.backdropped, top, &mut entries);
            entries.push(OrderEntry::Node(self.ensure_node(context, top)));
        }
        let spliced = splice(context.tx, root, &self.root_entries, &entries);
        self.root_entries = entries;
        spliced
    }

    /// Replaces `found` with the volatile boxes that are `marked` boxes or hold one below them, or
    /// with every volatile box when `coverage` is all of them.
    ///
    /// A child that sorts by the place of other boxes sorts by boxes below its owner, so only an
    /// order mark there can change its rank.
    fn volatile_above(
        &mut self,
        store: &LayoutStore,
        marked: &[BoxKey],
        coverage: ChangeCoverage,
        found: &mut Vec<BoxKey>,
    ) {
        found.clear();
        if coverage == ChangeCoverage::All {
            found.extend(self.volatile.iter().copied());
            return;
        }
        if self.volatile.is_empty() {
            return;
        }
        let seen = &mut self.scratch.seen;
        seen.clear();
        for &key in marked {
            let mut current = Some(key);
            while let Some(key) = current {
                if !seen.insert(key) {
                    break;
                }
                if self.volatile.contains(&key) {
                    found.push(key);
                }
                current = store.get(key).and_then(|node| node.parent);
            }
        }
    }

    /// Visits every box in `work` not yet visited in this sync, and returns the entries spliced.
    fn drain(&mut self, context: &mut Context<'_>, work: &mut Vec<BoxKey>) -> usize {
        let mut entries = 0;
        while let Some(key) = work.pop() {
            if self.scratch.visited.insert(key) {
                entries += self.visit(context, key, work);
            }
        }
        entries
    }

    /// Reports whether the box `key` keeps its node and lists its children in the order its
    /// node was built with.
    fn keeps_children(&self, store: &LayoutStore, key: BoxKey) -> bool {
        let Some(node) = self.nodes.get(&key) else {
            return false;
        };
        store.get(key).is_some()
            && (self.top == Some(key) || self.needs_node(store, key))
            && *stacking::children_in_paint_order(store, key) == *node.children
    }

    /// Brings the list that holds the box `key` up to date, and returns the entries spliced.
    ///
    /// A box listed as its own node rebuilds its node's list. A box whose place in its owner's
    /// list changed has the owner rebuilt.
    fn visit(&mut self, context: &mut Context<'_>, key: BoxKey, work: &mut Vec<BoxKey>) -> usize {
        if context.store.get(key).is_none() {
            return 0;
        }
        let wants_node = self.top == Some(key) || self.needs_node(context.store, key);
        if wants_node && self.nodes.contains_key(&key) {
            return self.rebuild(context, key);
        }
        if let Some(place) = self.placed.get(&key) {
            work.push(place.owner);
        }
        0
    }

    /// Reports whether the box `key` is listed as an order node of its own.
    fn needs_node(&self, store: &LayoutStore, key: BoxKey) -> bool {
        self.effects.contains_key(&key)
            || store.get(key).is_some_and(|node| !node.children.is_empty())
            || zgui_layout::fragment::stacking::establishes(store, key)
    }

    /// Returns the order node of the box `key`, creating it with its whole list on first use.
    ///
    /// A node created counts as visited by the sync.
    fn ensure_node(&mut self, context: &mut Context<'_>, key: BoxKey) -> OrderId {
        if let Some(node) = self.nodes.get(&key) {
            return node.id;
        }
        self.scratch.visited.insert(key);
        let id = context.tx.create_order(OrderNode::default());
        self.nodes.insert(
            key,
            Node {
                id,
                entries: Vec::new(),
                children: Vec::new(),
                effect: None,
            },
        );
        self.restyled.insert(key);
        self.created += self.rebuild(context, key);
        id
    }

    /// Rebuilds the list of the box `key`'s node, and returns the entries spliced.
    ///
    /// A list whose children and entries stay as they are writes nothing.
    fn rebuild(&mut self, context: &mut Context<'_>, key: BoxKey) -> usize {
        let store = context.store;
        let children = stacking::children_in_paint_order(store, key);
        let volatile = children
            .iter()
            .any(|&child| sorts_by_others(store, key, child));
        if volatile != self.volatile.contains(&key) {
            if volatile {
                self.volatile.insert(key);
            } else {
                self.volatile.remove(&key);
            }
        }
        // Taken while the list is built, since a child's new node builds its own list first.
        let mut entries = std::mem::take(&mut self.scratch.entries);
        entries.clear();
        push_chunks(store, key, &mut entries);
        for &child in children.iter() {
            push_backdrop(context.backdropped, child, &mut entries);
            if self.needs_node(store, child) {
                entries.push(OrderEntry::Node(self.ensure_node(context, child)));
            } else {
                push_chunks(store, child, &mut entries);
                push_outline(context.outlined, child, &mut entries);
            }
        }
        push_outline(context.outlined, key, &mut entries);

        let unchanged = self
            .nodes
            .get(&key)
            .is_none_or(|node| node.entries == entries && *node.children == *children);
        if unchanged {
            self.scratch.entries = entries;
            return 0;
        }
        for &child in children.iter() {
            let node = self.needs_node(store, child);
            self.placed.insert(child, Place { owner: key, node });
            if !node && self.nodes.contains_key(&child) {
                self.scratch.orphans.push(child);
            }
        }
        let Some(node) = self.nodes.get_mut(&key) else {
            return 0;
        };
        let old = std::mem::replace(&mut node.children, children.into_owned());
        let listed = Listed::of(&node.children);
        for old in old {
            if listed.holds(old) {
                continue;
            }
            if self
                .placed
                .get(&old)
                .is_some_and(|place| place.owner == key)
            {
                self.placed.remove(&old);
            }
            self.scratch.orphans.push(old);
        }
        let spliced = splice(context.tx, node.id, &node.entries, &entries);
        self.scratch.entries = std::mem::replace(&mut node.entries, entries);
        spliced
    }

    /// Writes the effects that changed onto the nodes of their boxes, and returns the number of
    /// writes.
    fn write_effects(&mut self, context: &mut Context<'_>) -> usize {
        let mut writes = 0;
        for key in std::mem::take(&mut self.restyled) {
            let Some(node) = self.nodes.get_mut(&key) else {
                continue;
            };
            let (effect, written) = effect::write(
                context.tx,
                context.mirror,
                context.tables,
                node.id,
                node.effect.take(),
                self.effects.get(&key),
            );
            node.effect = effect;
            writes += written;
        }
        writes
    }

    /// Removes the nodes no list names any more, with the nodes below them.
    fn remove_orphans(&mut self, context: &mut Context<'_>) {
        let mut orphans = std::mem::take(&mut self.scratch.orphans);
        while let Some(key) = orphans.pop() {
            let named =
                self.top == Some(key) || self.placed.get(&key).is_some_and(|place| place.node);
            if named {
                continue;
            }
            if context.store.get(key).is_none() {
                self.effects.remove(&key);
            }
            let Some(node) = self.nodes.remove(&key) else {
                continue;
            };
            self.volatile.remove(&key);
            context.tx.remove(node.id);
            if let Some(written) = node.effect {
                effect::remove(context.tx, context.mirror, written);
            }
            for child in node.children {
                if self
                    .placed
                    .get(&child)
                    .is_some_and(|place| place.owner == key)
                {
                    self.placed.remove(&child);
                    orphans.push(child);
                }
            }
        }
        self.scratch.orphans = orphans;
    }
}

/// The children of one list, for asking whether it holds a box.
enum Listed<'a> {
    /// A short list, searched in place.
    Few(&'a [BoxKey]),
    /// A long list, hashed.
    Many(FxHashSet<BoxKey>),
}

impl<'a> Listed<'a> {
    /// The longest list searched in place.
    const FEW: usize = 32;

    fn of(children: &'a [BoxKey]) -> Self {
        if children.len() <= Self::FEW {
            Self::Few(children)
        } else {
            Self::Many(children.iter().copied().collect())
        }
    }

    fn holds(&self, key: BoxKey) -> bool {
        match self {
            Self::Few(children) => children.contains(&key),
            Self::Many(children) => children.contains(&key),
        }
    }
}

/// Reports whether `child`, listed by `owner`, sorts among its siblings by the place of other
/// boxes: an out-of-flow box ranked where it was written, or a box listed by another than its
/// parent.
fn sorts_by_others(store: &LayoutStore, owner: BoxKey, child: BoxKey) -> bool {
    store.declared(child).is_some()
        || store
            .get(child)
            .is_some_and(|node| node.parent != Some(owner))
}

/// Appends the chunk of each fragment of the box `key`.
fn push_chunks(store: &LayoutStore, key: BoxKey, entries: &mut Vec<OrderEntry>) {
    let count = store.fragments_of_box(key).len() as u32;
    entries.extend(
        (0..count).map(|ordinal| OrderEntry::Chunk(key::at(key.as_u64(), ordinal, Part::Body))),
    );
}

/// Appends the outline chunk of the box `key`, when `outlined` says it has one.
fn push_outline(outlined: &dyn Fn(BoxKey) -> bool, key: BoxKey, entries: &mut Vec<OrderEntry>) {
    if outlined(key) {
        entries.push(OrderEntry::Chunk(key::at(key.as_u64(), 0, Part::Outline)));
    }
}

/// Appends the backdrop chunk of the box `key`, when `backdropped` says it has one.
fn push_backdrop(backdropped: &dyn Fn(BoxKey) -> bool, key: BoxKey, entries: &mut Vec<OrderEntry>) {
    if backdropped(key) {
        entries.push(OrderEntry::Chunk(key::at(key.as_u64(), 0, Part::Backdrop)));
    }
}
