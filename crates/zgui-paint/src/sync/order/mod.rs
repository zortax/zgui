//! The paint order mirrored as the compositor's order tree.
//!
//! Each box with children, each box that establishes a stacking context and each box drawn with a
//! group effect gets an order node. A node lists the chunks of its box's fragments, then each child
//! in painting order: a child with a node of its own as that node, any other child as the chunks
//! of its fragments.
//!
//! A box's list is rebuilt in these cases:
//!
//! - The layout store marks the order of its children as dirty.
//! - A child's place in it changed: a fragment came or went, or the child gained or lost its node.
//! - A child sorts by the place of other boxes, such as an absolutely positioned box ranked where
//!   it was written. No mark covers those boxes, so the list is rebuilt on every sync.
//!
//! A rebuilt list is written as one splice of the part that differs, and a list that did not
//! change writes nothing.

mod effect;

use compositor_types::{OrderEntry, OrderId, OrderNode, OutputId, Transaction};
use rustc_hash::{FxHashMap, FxHashSet};
use zgui_dom::side::BoxKey;
use zgui_layout::{LayoutStore, OrderVersion};
use zgui_scene::{ChangeCoverage, Scene};

use crate::emit::group::GroupEffect;
use crate::sync::key::{self, Part};
use crate::sync::mirror::Mirror;
use crate::walk::stacking;

/// The order node an output draws, and the order node of every box that has one.
#[derive(Debug, Default)]
pub(crate) struct Order {
    root: Option<OrderId>,
    top: Option<BoxKey>,
    root_entries: Vec<OrderEntry>,
    nodes: FxHashMap<BoxKey, Node>,
    placed: FxHashMap<BoxKey, Place>,
    effects: FxHashMap<BoxKey, GroupEffect>,
    version: Option<OrderVersion>,
    dirty: FxHashSet<BoxKey>,
    restyled: FxHashSet<BoxKey>,
    /// Entries spliced into the lists of nodes created during the current sync.
    created: usize,
    /// The boxes whose list holds a child that sorts by other boxes than itself, which no mark
    /// covers, so they are rebuilt on every sync.
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
}

/// What a sync of the order tree reads and writes besides the order tree.
pub(crate) struct Context<'a> {
    pub(crate) tx: &'a mut Transaction,
    pub(crate) store: &'a LayoutStore,
    pub(crate) mirror: &'a mut Mirror,
    pub(crate) scene: &'a mut Scene,
    /// Whether a box has an outline chunk.
    pub(crate) outlined: &'a dyn Fn(BoxKey) -> bool,
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
        work.extend(self.dirty.drain());
        work.extend(self.volatile.iter().copied());

        let mut entries = self.sync_root(context, root);
        let mut visited = FxHashSet::default();
        while let Some(key) = work.pop() {
            if visited.insert(key) {
                entries += self.visit(context, key, &mut work);
            }
        }
        entries += std::mem::take(&mut self.created);
        let effects = self.write_effects(context);
        self.remove_orphans(context);
        self.scratch.marked = work;
        Synced { entries, effects }
    }

    /// Lists the layout's root box in the output's root node, and returns the entries spliced.
    fn sync_root(&mut self, context: &mut Context<'_>, root: OrderId) -> usize {
        let top = context
            .store
            .root()
            .filter(|top| context.store.get(*top).is_some());
        if top == self.top && top.is_none_or(|top| self.nodes.contains_key(&top)) {
            return 0;
        }
        if let Some(old) = self.top.take() {
            self.scratch.orphans.push(old);
        }
        self.top = top;
        let entries: Vec<OrderEntry> = top
            .map(|top| OrderEntry::Node(self.ensure_node(context, top)))
            .into_iter()
            .collect();
        let spliced = splice(context.tx, root, &self.root_entries, &entries);
        self.root_entries = entries;
        spliced
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
    fn ensure_node(&mut self, context: &mut Context<'_>, key: BoxKey) -> OrderId {
        if let Some(node) = self.nodes.get(&key) {
            return node.id;
        }
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
    fn rebuild(&mut self, context: &mut Context<'_>, key: BoxKey) -> usize {
        let store = context.store;
        let mut entries = Vec::new();
        push_chunks(store, key, &mut entries);
        let children = stacking::children_in_paint_order(store, key).into_owned();
        if children
            .iter()
            .any(|&child| sorts_by_others(store, key, child))
        {
            self.volatile.insert(key);
        } else {
            self.volatile.remove(&key);
        }
        for &child in &children {
            let node = self.needs_node(store, child);
            self.placed.insert(child, Place { owner: key, node });
            if node {
                entries.push(OrderEntry::Node(self.ensure_node(context, child)));
            } else {
                push_chunks(store, child, &mut entries);
                push_outline(context.outlined, child, &mut entries);
                if self.nodes.contains_key(&child) {
                    self.scratch.orphans.push(child);
                }
            }
        }
        push_outline(context.outlined, key, &mut entries);

        let Some(node) = self.nodes.get_mut(&key) else {
            return 0;
        };
        let kept: FxHashSet<BoxKey> = children.iter().copied().collect();
        for old in std::mem::replace(&mut node.children, children) {
            if kept.contains(&old) {
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
        node.entries = entries;
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
                context.scene,
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

/// Replaces `old` with `new` as the list of `node`, with one splice of the part between their
/// common prefix and suffix, and returns the number of entries removed and inserted.
fn splice(tx: &mut Transaction, node: OrderId, old: &[OrderEntry], new: &[OrderEntry]) -> usize {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let removed = old.len() - prefix - suffix;
    let inserted = &new[prefix..new.len() - suffix];
    if removed > 0 || !inserted.is_empty() {
        tx.splice(node, prefix as u32, removed as u32, inserted);
    }
    removed + inserted.len()
}
