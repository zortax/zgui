//! Paint sources mirrored as paint slots.

use compositor_types::{Color, Paint, PaintId, PaintKind, Transaction};
use rustc_hash::FxHashMap;
use zgui_scene::{ChangeCoverage, PaintTable, TableVersion};

use crate::sync::mirror::color;

/// The paint slot of each mirrored zgui paint, by zgui paint id, and one slot per solid colour
/// glyphs are drawn in.
///
/// A mirrored paint is held in the zgui table, so its id names the same paint until the mirror is
/// reset.
#[derive(Debug, Default)]
pub(crate) struct Paints {
    slots: Vec<Option<Option<(PaintId, PaintKind)>>>,
    written: Vec<Option<Paint>>,
    solids: FxHashMap<[u32; 4], PaintId>,
    version: Option<TableVersion>,
    changed: Vec<zgui_scene::PaintId>,
}

impl Paints {
    /// Returns the slot and kind of the zgui paint `id`, mirroring it on first use.
    ///
    /// An image paint, which the compositor has no slot for, is `None`.
    pub(crate) fn ensure(
        &mut self,
        tx: &mut Transaction,
        table: &mut PaintTable,
        id: zgui_scene::PaintId,
    ) -> Option<(PaintId, PaintKind)> {
        if self.version.is_none() {
            self.version = Some(table.version());
        }
        let index = id.0 as usize;
        if let Some(Some(mirrored)) = self.slots.get(index) {
            return *mirrored;
        }
        let paint = table.get(id).and_then(color::paint);
        let mirrored = paint
            .clone()
            .map(|paint| (paint.kind(), tx.create_paint(paint)))
            .map(|(kind, slot)| (slot, kind));
        table.retain(id);
        if self.slots.len() <= index {
            self.slots.resize(index + 1, None);
            self.written.resize(index + 1, None);
        }
        self.slots[index] = Some(mirrored);
        self.written[index] = paint;
        mirrored
    }

    /// Returns the slot that holds the solid `color`, creating it on first use.
    pub(crate) fn solid(&mut self, tx: &mut Transaction, color: Color) -> PaintId {
        *self
            .solids
            .entry(color.0.map(f32::to_bits))
            .or_insert_with(|| tx.create_paint(Paint::Solid(color)))
    }

    /// Rewrites the mirrored paints whose value the table changed since the last call.
    ///
    /// A paint whose kind changed gets a new slot, which the chunks recorded after it use.
    /// Returns the number of slots written.
    pub(crate) fn sync(&mut self, tx: &mut Transaction, table: &PaintTable) -> usize {
        let Some(version) = self.version else {
            return 0;
        };
        self.changed.clear();
        let coverage = table.changes_since(version, &mut self.changed);
        self.version = Some(table.version());
        if coverage == ChangeCoverage::All {
            self.changed.clear();
            self.changed
                .extend((0..self.slots.len() as u32).map(zgui_scene::PaintId));
        }
        let mut written = 0;
        for &id in &self.changed {
            let Some(Some(Some((slot, kind)))) = self.slots.get(id.0 as usize).copied() else {
                continue;
            };
            let Some(paint) = table.get(id).and_then(color::paint) else {
                continue;
            };
            if self.written[id.0 as usize].as_ref() == Some(&paint) {
                continue;
            }
            self.written[id.0 as usize] = Some(paint.clone());
            if paint.kind() == kind {
                tx.set_paint(slot, paint);
            } else {
                // The old slot stays, since recorded chunks may still refer to it.
                let kind = paint.kind();
                self.slots[id.0 as usize] = Some(Some((tx.create_paint(paint), kind)));
            }
            written += 1;
        }
        written
    }
}
