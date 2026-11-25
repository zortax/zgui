//! The paint order mirrored as the compositor's order tree.
//!
//! The tree holds the boxes that are drawn: those with a chunk recorded in their subtree and those
//! drawn with a group effect. Each drawn box with children, each that establishes a stacking
//! context and each drawn with a group effect gets an order node. A node lists the chunks of its
//! box's fragments, then each drawn child in painting order: a child with a node of its own as that
//! node, any other child as the chunks of its fragments. A child's backdrop chunk comes right
//! before it, outside its node.
//!
//! A box's list is rebuilt in these cases:
//!
//! - The layout store marks the order of its children as dirty.
//! - A child's place in it changed: a fragment came or went, or the child gained or lost its node.
//! - A child sorts by the place of other boxes, such as an absolutely positioned box ranked where
//!   it was written. No mark covers those boxes, so the list is checked after a sync that saw an
//!   order mark below its box.
//! - A box below it is drawn for the first time, or a box it now holds was dropped from another
//!   list.
//!
//! A rebuilt list is written as one splice per run of entries that differs, and a list that did
//! not change writes nothing.

mod drawn;
mod effect;
mod ends;
mod splice;

use crate::Tables;
use iceblink::{OrderEntry, OrderId, OrderNode, OutputId, Transaction};
use rustc_hash::{FxHashMap, FxHashSet};
use zgui_dom::side::BoxKey;
use zgui_layout::{JournalVersion, LayoutStore};
use zgui_space::ChangeCoverage;

use crate::emit::group::GroupEffect;
use crate::sync::key::{self, Part};
use crate::sync::mirror::Mirror;
use crate::sync::names::Names;
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
    /// The boxes drawn since the last sync that may not be listed yet.
    drawn: Vec<BoxKey>,
    scratch: Scratch,
}

/// The order node of one box.
#[derive(Debug)]
struct Node {
    id: OrderId,
    entries: Vec<OrderEntry>,
    children: Vec<BoxKey>,
    /// Where the entries of each child start in `entries`, and where those of the last child end.
    spans: Vec<u32>,
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
    spans: Vec<u32>,
    children: Vec<BoxKey>,
    /// The boxes the current sync lists for the first time, with the boxes above them that are
    /// not listed yet.
    shown: FxHashSet<BoxKey>,
    /// The boxes the current sync was given to visit.
    touched: FxHashSet<BoxKey>,
    /// Whether the current sync visits every node.
    whole: bool,
}

/// What a sync of the order tree reads and writes besides the order tree.
pub(crate) struct Context<'a> {
    pub(crate) tx: &'a mut Transaction,
    pub(crate) store: &'a LayoutStore,
    pub(crate) mirror: &'a mut Mirror,
    pub(crate) tables: &'a mut Tables,
    /// The names the boxes' chunks are recorded under.
    pub(crate) names: &'a Names,
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

    /// Reports whether a list or a node of the order tree holds the box `key`.
    pub(crate) fn knows(&self, key: BoxKey) -> bool {
        self.placed.contains_key(&key) || self.nodes.contains_key(&key)
    }

    /// Returns the box whose list holds the box `key`, and the place of `key` among its
    /// children.
    pub(crate) fn place_of(&self, key: BoxKey) -> Option<(BoxKey, u32)> {
        let owner = self.placed.get(&key)?.owner;
        let children = self
            .nodes
            .get(&owner)
            .map_or(&[][..], |node| &node.children[..]);
        let place = children.iter().position(|child| *child == key)?;
        Some((owner, place as u32))
    }

    /// Gives the box `key` the order node of the box `gone`, which left.
    ///
    /// The node keeps its list until the next sync rebuilds it for `key`.
    pub(crate) fn take_over(&mut self, key: BoxKey, gone: BoxKey) {
        self.effects.remove(&gone);
        self.restyled.remove(&gone);
        let Some(node) = self.nodes.remove(&gone) else {
            return;
        };
        for child in &node.children {
            if let Some(place) = self.placed.get_mut(child)
                && place.owner == gone
            {
                place.owner = key;
            }
        }
        if self.volatile.remove(&gone) {
            self.volatile.insert(key);
        }
        if node.effect.is_some() {
            self.restyled.insert(key);
        }
        self.nodes.insert(key, node);
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
        if effect.is_some() {
            self.recorded(key);
        }
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
        self.show_drawn(context.store, &mut work);
        self.scratch.touched.clear();
        self.scratch.touched.extend(work.iter().copied());
        self.scratch.whole = coverage == ChangeCoverage::All;

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
        entries += self.relist_dropped(context, &mut work);
        entries += std::mem::take(&mut self.created);
        let effects = self.write_effects(context);
        self.remove_orphans(context);
        self.scratch.shown.clear();
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
            push_backdrop(context, top, &mut entries);
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
            // A box no list holds has no list to bring up to date.
            if self.knows(key) && self.scratch.visited.insert(key) {
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
            && stacking::children_in_paint_order(store, key)
                .iter()
                .copied()
                .filter(|&child| self.shows(child))
                .eq(node.children.iter().copied())
    }

    /// Brings the list that holds the box `key` up to date, and returns the entries spliced.
    ///
    /// A box listed as its own node rebuilds its node's list. A box whose place in its owner's
    /// list changed has the owner rebuilt.
    fn visit(&mut self, context: &mut Context<'_>, key: BoxKey, work: &mut Vec<BoxKey>) -> usize {
        // A removed box's node goes with the list of the live box that held it.
        if context.store.get(key).is_none() || context.store.removed(key) {
            return 0;
        }
        if self.nodes.contains_key(&key)
            && (self.top == Some(key) || self.needs_node(context.store, key))
        {
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
                spans: Vec::new(),
                effect: None,
            },
        );
        self.restyled.insert(key);
        self.created += self.rebuild(context, key);
        id
    }

    /// Rebuilds the list of the box `key`'s node from its drawn children, and returns the entries
    /// spliced.
    fn rebuild(&mut self, context: &mut Context<'_>, key: BoxKey) -> usize {
        let store = context.store;
        // Taken while the list is built, since a child's new node builds its own list first.
        let mut children = std::mem::take(&mut self.scratch.children);
        children.clear();
        children.extend(
            stacking::children_in_paint_order(store, key)
                .iter()
                .copied()
                .filter(|&child| self.shows(child)),
        );
        let spliced = self.rebuild_from(context, key, &children);
        self.scratch.children = children;
        spliced
    }

    /// Rebuilds the list of the box `key`'s node with `children`, and returns the entries spliced.
    ///
    /// A list that keeps a run of its children unchanged is rebuilt at its ends, see
    /// [`Order::rebuild_ends`]. A list whose children and entries stay as they are writes
    /// nothing.
    fn rebuild_from(
        &mut self,
        context: &mut Context<'_>,
        key: BoxKey,
        children: &[BoxKey],
    ) -> usize {
        let store = context.store;
        if let Some(spliced) = self.rebuild_ends(context, key, children) {
            return spliced;
        }
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
        let mut entries = std::mem::take(&mut self.scratch.entries);
        entries.clear();
        let mut spans = std::mem::take(&mut self.scratch.spans);
        spans.clear();
        push_chunks(context.names, store, key, &mut entries);
        for &child in children {
            spans.push(entries.len() as u32);
            self.push_child(context, child, &mut entries);
        }
        spans.push(entries.len() as u32);
        push_outline(context, key, &mut entries);

        let Some(node) = self.nodes.get_mut(&key) else {
            self.scratch.entries = entries;
            self.scratch.spans = spans;
            return 0;
        };
        std::mem::swap(&mut node.spans, &mut spans);
        self.scratch.spans = spans;
        if node.entries == entries && *node.children == *children {
            self.scratch.entries = entries;
            return 0;
        }
        for &child in children {
            let node = self.needs_node(store, child);
            self.placed.insert(child, Place { owner: key, node });
            if !node && self.nodes.contains_key(&child) {
                self.scratch.orphans.push(child);
            }
        }
        let Some(node) = self.nodes.get_mut(&key) else {
            return 0;
        };
        let old = std::mem::replace(&mut node.children, children.to_vec());
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

    /// Appends the entries `child` takes in the list of the node that lists it.
    fn push_child(
        &mut self,
        context: &mut Context<'_>,
        child: BoxKey,
        entries: &mut Vec<OrderEntry>,
    ) {
        push_backdrop(context, child, entries);
        if self.needs_node(context.store, child) {
            entries.push(OrderEntry::Node(self.ensure_node(context, child)));
        } else {
            push_chunks(context.names, context.store, child, entries);
            push_outline(context, child, entries);
        }
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
            if context.store.get(key).is_none() || context.store.removed(key) {
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
fn push_chunks(names: &Names, store: &LayoutStore, key: BoxKey, entries: &mut Vec<OrderEntry>) {
    let name = names.of(key);
    let count = store.fragments_of_box(key).len() as u32;
    entries.extend((0..count).map(|ordinal| OrderEntry::Chunk(key::at(name, ordinal, Part::Body))));
}

/// Appends the outline chunk of the box `key`, when it has one.
fn push_outline(context: &Context<'_>, key: BoxKey, entries: &mut Vec<OrderEntry>) {
    if (context.outlined)(key) {
        let name = context.names.of(key);
        entries.push(OrderEntry::Chunk(key::at(name, 0, Part::Outline)));
    }
}

/// Appends the backdrop chunk of the box `key`, when it has one.
fn push_backdrop(context: &Context<'_>, key: BoxKey, entries: &mut Vec<OrderEntry>) {
    if (context.backdropped)(key) {
        let name = context.names.of(key);
        entries.push(OrderEntry::Chunk(key::at(name, 0, Part::Backdrop)));
    }
}
