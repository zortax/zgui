//! Coordinate systems mirrored as transform nodes.

mod drive;
mod thumb;
mod written;

pub use self::drive::ScrollDrive;

use iceblink::{ScrollId, Transaction, TransformId};
use rustc_hash::FxHashMap;
use zgui_space::ChangeCoverage;
use zgui_space::spatial::{PropertyOwner, SpatialId, SpatialNode, SpatialTree, TreeVersion};

use crate::sync::mirror::holds::Holds;

use self::drive::Driven;
use self::written::{Scroll, Written};

/// The transform node of each mirrored coordinate system, by slot.
///
/// A mirrored node has the node of the coordinate system above it as its parent, so a write to
/// one coordinate system moves everything below it. The content of a scroll container applies its
/// scroll offset through a scroll node, so a scroll writes the scroll node's offset.
///
/// A slot that another coordinate system takes over gets a new node. The old node stays while a
/// chunk or a node holds it.
#[derive(Debug, Default)]
pub(crate) struct Spaces {
    slots: Vec<Option<Entry>>,
    nodes: FxHashMap<TransformId, u32>,
    scrolls: FxHashMap<ScrollId, Scrolled>,
    adopted: FxHashMap<ScrollId, u64>,
    version: Option<TreeVersion>,
    changed: Vec<u32>,
    /// How the compositor scrolls the nodes on its own, or `None` for client writes only.
    drive: Option<ScrollDrive>,
    /// The content of the container no gesture may scroll.
    frozen: Option<PropertyOwner>,
    /// Whether every node is written again at the next sync, because the drive or the frozen
    /// container changed.
    restate: bool,
    /// The thumb nodes, by the slot of the content whose offset moves them and their axis.
    thumbs: FxHashMap<(u32, u8), thumb::Thumb>,
    thumb_nodes: FxHashMap<TransformId, (u32, u8)>,
}

/// One mirrored slot.
#[derive(Clone, Copy, Debug)]
struct Entry {
    /// The coordinate system the node mirrors, or `None` for a slot that resolves to nothing.
    occupant: Option<SpatialId>,
    id: TransformId,
    scroll: Option<ScrollId>,
    written: Written,
    /// What the scroll node's behaviour was written from.
    driven: Option<Driven>,
}

/// The container a scroll node scrolls, and the slot of its content.
#[derive(Clone, Copy, Debug)]
struct Scrolled {
    container: PropertyOwner,
    slot: u32,
}

impl Spaces {
    /// Returns the node of `slot` for the chunk being recorded, mirroring it on first use.
    pub(crate) fn ensure(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        tree: &SpatialTree,
        slot: u32,
    ) -> TransformId {
        let id = self.mirror(tx, holds, tree, slot);
        holds.name(id);
        id
    }

    /// Returns the node of `slot`, mirroring it and the coordinate systems above it on first use.
    ///
    /// A slot that resolves to nothing is mirrored as a root at the identity.
    pub(crate) fn mirror(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        tree: &SpatialTree,
        slot: u32,
    ) -> TransformId {
        let occupant = live(tree, slot);
        if let Some(Some(entry)) = self.slots.get(slot as usize)
            && entry.occupant == occupant
        {
            return entry.id;
        }
        self.forget(slot);

        let (node, parent, parent_scroll) = self.value(tx, holds, tree, occupant);
        let written = Written::of(node.as_ref(), parent, parent_scroll);
        let driven = self.driven(tree, slot, &written);
        let scroll = self.add_scroll(tx, holds, tree, slot, &written, driven);
        let id = tx.create_transform(written.node(scroll));
        holds.created(id);
        if let Some(parent) = parent {
            holds.hold(id, parent);
        }
        if let Some(scroll) = scroll {
            holds.hold(id, scroll);
        }
        if let Some(sticky) = written.sticky {
            holds.hold(id, sticky.scroll);
        }

        let index = slot as usize;
        if self.slots.len() <= index {
            self.slots.resize(index + 1, None);
        }
        self.slots[index] = Some(Entry {
            occupant,
            id,
            scroll,
            written,
            driven,
        });
        self.nodes.insert(id, slot);
        id
    }

    /// Writes every mirrored slot whose coordinate system changed since the last call.
    ///
    /// Returns the number of nodes written.
    pub(crate) fn sync(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        tree: &SpatialTree,
    ) -> usize {
        let mut changed = std::mem::take(&mut self.changed);
        changed.clear();
        let coverage = match self.version {
            Some(_) if std::mem::take(&mut self.restate) => ChangeCoverage::All,
            Some(version) => tree.changes_since(version, &mut changed),
            None => ChangeCoverage::All,
        };
        self.version = Some(tree.version());
        if coverage == ChangeCoverage::All {
            changed.clear();
            changed.extend(self.nodes.values().copied());
        }
        changed.sort_unstable();
        changed.dedup();

        let mut written = 0;
        for &slot in &changed {
            written += self.rewrite(tx, holds, tree, slot);
        }
        self.changed = changed;
        written + self.acknowledge(tx)
    }

    /// Returns the scroll node that applies the scroll offset of `container`.
    pub(crate) fn scroll_of(
        &self,
        tree: &SpatialTree,
        container: PropertyOwner,
    ) -> Option<ScrollId> {
        let content = tree.of(container.content())?;
        let entry = self.slots.get(content.index() as usize)?.as_ref()?;
        entry.scroll.filter(|_| entry.occupant == Some(content))
    }

    /// Returns the container whose scroll offset `scroll` applies.
    pub(crate) fn container_of(&self, scroll: ScrollId) -> Option<PropertyOwner> {
        self.scrolls.get(&scroll).map(|scrolled| scrolled.container)
    }

    /// Makes the next write of the offset of `scroll` adopt the compositor's scrolling up to
    /// `serial`.
    pub(crate) fn adopt(&mut self, scroll: ScrollId, serial: u64) {
        if self.scrolls.contains_key(&scroll) {
            self.adopted.insert(scroll, serial);
        }
    }

    /// Makes the compositor scroll every node with `drive`, or only by client writes for `None`.
    pub(crate) fn set_drive(&mut self, drive: Option<ScrollDrive>) {
        if self.drive != drive {
            self.drive = drive;
            self.restate = true;
        }
    }

    /// Stops gestures from scrolling the content `frozen`, and lets them scroll every other one.
    pub(crate) fn set_frozen(&mut self, frozen: Option<PropertyOwner>) {
        if self.frozen != frozen {
            self.frozen = frozen;
            self.restate = true;
        }
    }

    /// Returns what the behaviour of the scroll node of `slot`, holding `written`, is written from.
    fn driven(&self, tree: &SpatialTree, slot: u32, written: &Written) -> Option<Driven> {
        let drive = self.drive?;
        let scroll = written.scroll?;
        // The slot of a scroll node is its content's, whose owner the frozen one is.
        let frozen = self.frozen.is_some() && tree.owner_at(slot) == self.frozen;
        Some(Driven {
            drive,
            user: if frozen { [false; 2] } else { scroll.user },
            frozen,
        })
    }

    /// Forgets the node `id`, which was removed.
    pub(crate) fn removed(&mut self, id: TransformId) {
        if self.removed_thumb(id) {
            return;
        }
        if let Some(slot) = self.nodes.remove(&id) {
            self.slots[slot as usize] = None;
        }
    }

    /// Forgets the scroll node `id`, which was removed.
    pub(crate) fn removed_scroll(&mut self, id: ScrollId) {
        self.scrolls.remove(&id);
        self.adopted.remove(&id);
    }

    /// Returns the value of `occupant`, the mirrored node of its parent and the scroll node that
    /// parent applies.
    ///
    /// A coordinate system whose parent is gone resolves to nothing.
    fn value(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        tree: &SpatialTree,
        occupant: Option<SpatialId>,
    ) -> (Option<SpatialNode>, Option<TransformId>, Option<ScrollId>) {
        let Some(node) = occupant.and_then(|id| tree.get(id)).copied() else {
            return (None, None, None);
        };
        match node.parent {
            None => (Some(node), None, None),
            Some(parent) if tree.contains(parent) => {
                let mirrored = self.mirror(tx, holds, tree, parent.index());
                let scroll = self
                    .slots
                    .get(parent.index() as usize)
                    .and_then(|entry| entry.as_ref()?.scroll);
                (Some(node), Some(mirrored), scroll)
            }
            Some(_) => (None, None, None),
        }
    }

    /// Writes the node of `slot` where its coordinate system's value changed, and returns the
    /// number of nodes written.
    fn rewrite(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        tree: &SpatialTree,
        slot: u32,
    ) -> usize {
        let Some(Some(entry)) = self.slots.get(slot as usize).copied() else {
            return 0;
        };
        let occupant = live(tree, slot);
        if occupant != entry.occupant {
            // The chunks that name the old node are recorded again for the new occupant.
            self.forget(slot);
            return 0;
        }
        let (node, parent, parent_scroll) = self.value(tx, holds, tree, occupant);
        let next = Written::of(node.as_ref(), parent, parent_scroll);
        let driven = self.driven(tree, slot, &next);
        if next == entry.written && driven == entry.driven {
            return 0;
        }

        let mut stored = entry;
        stored.written = next;
        stored.driven = driven;
        let mut writes = 0;
        if next.parent != entry.written.parent {
            if let Some(old) = entry.written.parent {
                holds.unhold(entry.id, old);
            }
            if let Some(new) = next.parent {
                holds.hold(entry.id, new);
            }
        }
        let (stuck, sticks) = (
            entry.written.sticky.map(|sticky| sticky.scroll),
            next.sticky.map(|sticky| sticky.scroll),
        );
        if stuck != sticks {
            if let Some(old) = stuck {
                holds.unhold(entry.id, old);
            }
            if let Some(new) = sticks {
                holds.hold(entry.id, new);
            }
        }
        let reshaped = next.parent != entry.written.parent
            || next.scroll.is_some() != entry.written.scroll.is_some();
        if reshaped {
            self.drop_scroll(holds, &entry);
            stored.scroll = self.add_scroll(tx, holds, tree, slot, &next, driven);
            if let Some(scroll) = stored.scroll {
                holds.hold(entry.id, scroll);
            }
            tx.set_transform_node(entry.id, next.node(stored.scroll));
            writes += 1;
        } else {
            if next.local != entry.written.local
                || next.fixed != entry.written.fixed
                || next.sticky != entry.written.sticky
            {
                tx.set_transform_node(entry.id, next.node(entry.scroll));
                writes += 1;
            }
            if let (Some(id), Some(old), Some(new), Some(space)) =
                (entry.scroll, entry.written.scroll, next.scroll, next.parent)
            {
                let behaved = (entry.driven != driven).then_some(driven);
                writes += self.write_scroll(tx, id, (&old, &new), space, behaved);
            }
        }
        self.slots[slot as usize] = Some(stored);
        writes
    }

    /// Creates the scroll node the node of `slot` applies with the value `written`, if it has one,
    /// with the behaviour written from `driven`.
    fn add_scroll(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        tree: &SpatialTree,
        slot: u32,
        written: &Written,
        driven: Option<Driven>,
    ) -> Option<ScrollId> {
        let (scroll, space) = written.scroll.zip(written.parent)?;
        let id = tx.create_scroll(scroll.node(space, driven.map(Driven::behaviour)));
        holds.created(id);
        holds.hold(id, space);
        if let Some(owner) = tree.owner_at(slot) {
            self.scrolls.insert(
                id,
                Scrolled {
                    container: owner.content(),
                    slot,
                },
            );
        }
        Some(id)
    }

    /// Ends the hold of `entry` on its scroll node, if it has one.
    fn drop_scroll(&mut self, holds: &mut Holds, entry: &Entry) {
        if let Some(old) = entry.scroll {
            holds.unhold(entry.id, old);
            self.scrolls.remove(&old);
            self.adopted.remove(&old);
        }
    }

    /// Writes what changed between two values `(old, new)` of the scroll node `id`, and returns
    /// the number of writes.
    ///
    /// `behaved` holds the new behaviour's source when the behaviour changed. A node whose port,
    /// content or behaviour changed is written whole at its base offset, so the compositor's
    /// motion goes on. An adopted offset becomes that base first. A node that became frozen is then
    /// scrolled to that offset, which ends its motion.
    fn write_scroll(
        &mut self,
        tx: &mut Transaction,
        id: ScrollId,
        (old, new): (&Scroll, &Scroll),
        space: TransformId,
        behaved: Option<Option<Driven>>,
    ) -> usize {
        let driven = behaved.unwrap_or_else(|| self.driven_of(id));
        if old.reshaped(new) || behaved.is_some() {
            let mut writes = 1;
            if let Some(serial) = self.adopted.remove(&id) {
                tx.acknowledge_scroll(id, serial, new.offset);
                writes += 1;
            }
            tx.set_scroll_node(id, new.node(space, driven.map(Driven::behaviour)));
            let froze = behaved.is_some_and(|behaved| behaved.is_some_and(|driven| driven.frozen))
                && !self.driven_of(id).is_some_and(|driven| driven.frozen);
            if froze {
                tx.scroll_to(id, new.offset);
                writes += 1;
            }
            return writes;
        }
        if old.offset == new.offset {
            return 0;
        }
        match self.adopted.remove(&id) {
            Some(serial) => tx.acknowledge_scroll(id, serial, new.offset),
            None => tx.scroll_to(id, new.offset),
        }
        1
    }

    /// Returns what the behaviour of the scroll node `id` was last written from.
    fn driven_of(&self, id: ScrollId) -> Option<Driven> {
        let scrolled = self.scrolls.get(&id)?;
        self.slots.get(scrolled.slot as usize)?.as_ref()?.driven
    }

    /// Writes the adoptions whose offset did not change, and returns the number of writes.
    fn acknowledge(&mut self, tx: &mut Transaction) -> usize {
        let mut writes = 0;
        for (id, serial) in self.adopted.drain() {
            let scroll = self
                .scrolls
                .get(&id)
                .and_then(|scrolled| self.slots.get(scrolled.slot as usize)?.as_ref())
                .and_then(|entry| entry.written.scroll);
            if let Some(scroll) = scroll {
                tx.acknowledge_scroll(id, serial, scroll.offset);
                writes += 1;
            }
        }
        writes
    }

    /// Unlinks the node of `slot` from the slot, leaving it to the chunks and nodes that hold it.
    fn forget(&mut self, slot: u32) {
        let Some(entry) = self.slots.get_mut(slot as usize).and_then(Option::take) else {
            return;
        };
        self.nodes.remove(&entry.id);
        if let Some(scroll) = entry.scroll {
            self.scrolls.remove(&scroll);
            self.adopted.remove(&scroll);
        }
    }
}

/// Returns the live coordinate system occupying `slot`.
fn live(tree: &SpatialTree, slot: u32) -> Option<SpatialId> {
    tree.at(slot).filter(|id| tree.contains(*id))
}
