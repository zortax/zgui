//! Paint sources mirrored as paint slots.

use compositor_types::{Paint, PaintId, PaintKind, Transaction};
use zgui_scene::{ChangeCoverage, PaintTable, TableVersion};

use crate::sync::mirror::color;
use crate::sync::mirror::holds::Holds;

/// The paint slot of each mirrored zgui paint, by zgui paint id.
///
/// A mirrored paint is held in the zgui table while its slot exists, so its id names the same
/// paint.
#[derive(Debug, Default)]
pub(crate) struct Paints {
    slots: Vec<Option<Option<(PaintId, PaintKind)>>>,
    written: Vec<Option<Paint>>,
    version: Option<TableVersion>,
    changed: Vec<zgui_scene::PaintId>,
}

/// One mirrored paint.
#[derive(Clone, Debug)]
struct Entry {
    slot: PaintId,
    written: Paint,
}

impl Paints {
    /// Returns the slot and kind of the zgui paint `id` for the chunk being recorded, mirroring it
    /// on first use.
    ///
    /// An image paint, which the compositor has no slot for, is `None`.
    pub(crate) fn ensure(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        table: &mut PaintTable,
        id: zgui_scene::PaintId,
    ) -> Option<(PaintId, PaintKind)> {
        if self.version.is_none() {
            self.version = Some(table.version());
        }
        let index = id.0 as usize;
        if let Some(Some(entry)) = self.entries.get(index) {
            holds.name(entry.slot);
            return Some((entry.slot, entry.written.kind()));
        }
        let paint = table.get(id).and_then(color::paint)?;
        let slot = tx.create_paint(paint.clone());
        holds.created(slot);
        holds.name(slot);
        table.retain(id);
        if self.entries.len() <= index {
            self.entries.resize(index + 1, None);
        }
        self.slots[index] = Some(mirrored);
        self.written[index] = paint;
        mirrored
    }

    /// Rewrites the mirrored paints whose value the table changed since the last call.
    ///
    /// A paint whose kind changed gets a new slot, which the chunks recorded after it use. The old
    /// slot stays while a chunk holds it. Returns the number of slots written.
    pub(crate) fn sync(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        table: &PaintTable,
    ) -> usize {
        let Some(version) = self.version else {
            return 0;
        };
        let mut changed = std::mem::take(&mut self.changed);
        changed.clear();
        let coverage = table.changes_since(version, &mut changed);
        self.version = Some(table.version());
        if coverage == ChangeCoverage::All {
            changed.clear();
            changed.extend(self.slots.values().map(|&raw| zgui_scene::PaintId(raw)));
        }
        let mut written = 0;
        for &id in &changed {
            written += self.rewrite(tx, holds, table, id);
        }
        self.changed = changed;
        written
    }

    /// Forgets the slot `id`, which was removed, and gives its paint's hold back to `table`.
    pub(crate) fn removed(&mut self, id: PaintId, table: &mut PaintTable) {
        if let Some(raw) = self.slots.remove(&id) {
            self.entries[raw as usize] = None;
            table.release(zgui_scene::PaintId(raw));
        }
    }

    /// Returns the zgui paints of every mirrored slot.
    pub(crate) fn mirrored(&self) -> impl Iterator<Item = zgui_scene::PaintId> + '_ {
        self.slots.values().map(|&raw| zgui_scene::PaintId(raw))
    }

    /// Writes the slot of `id` if its value changed, and returns the number of slots written.
    fn rewrite(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        table: &PaintTable,
        id: zgui_scene::PaintId,
    ) -> usize {
        let index = id.0 as usize;
        let Some(Some(entry)) = self.entries.get(index) else {
            return 0;
        };
        let Some(paint) = table.get(id).and_then(color::paint) else {
            return 0;
        };
        if entry.written == paint {
            return 0;
        }
        let mut slot = entry.slot;
        if paint.kind() == entry.written.kind() {
            tx.set_paint(slot, paint.clone());
        } else {
            self.slots.remove(&slot);
            slot = tx.create_paint(paint.clone());
            holds.created(slot);
            self.slots.insert(slot, id.0);
        }
        self.entries[index] = Some(Entry {
            slot,
            written: paint,
        });
        1
    }
}
