//! Which sheets are installed, in what order, and what happens when a handle is dropped.

use std::sync::{Arc, Mutex};

use style::shared_lock::SharedRwLockReadGuard;
use style::stylesheets::DocumentStyleSheet;
use style::stylist::Stylist;

use crate::sheets::origin::SheetOrigin;

/// Which installed sheet a handle refers to.
///
/// Allocated in installation order and never reused, so a handle for a sheet that has been removed
/// names nothing rather than naming whatever was installed next.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SheetId(u64);

/// A stylesheet installed in a rule set.
///
/// Dropping the handle removes the sheet. That is the whole lifetime rule: a component library
/// keeps its handles for as long as it is loaded, and a scoped sheet is removed by letting its
/// handle go out of scope rather than by remembering to unregister it.
///
/// Removal is *recorded* by the drop and applied by the rule set at the start of the next frame
/// that asks whether its sheets changed. A handle may therefore be dropped from anywhere,
/// including from a place that has no access to the rule set at all.
pub struct SheetHandle {
    /// Which sheet this refers to.
    id: SheetId,
    /// Where the removal is recorded.
    dropped: Arc<DroppedSheets>,
}

impl SheetHandle {
    /// Which sheet this refers to.
    pub fn id(&self) -> SheetId {
        self.id
    }
}

impl core::fmt::Debug for SheetHandle {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("SheetHandle")
            .field("id", &self.id)
            .finish()
    }
}

impl Drop for SheetHandle {
    fn drop(&mut self) {
        self.dropped.record(self.id);
    }
}

/// The handles that have been dropped and whose sheets are still installed.
///
/// Shared with every live handle, so a drop on any thread is recorded without reaching the rule
/// set, which is not shareable.
#[derive(Debug, Default)]
pub(crate) struct DroppedSheets {
    /// The identifiers, in the order they were dropped.
    ids: Mutex<Vec<SheetId>>,
}

impl DroppedSheets {
    /// Records that the sheet `id` names is to be removed.
    fn record(&self, id: SheetId) {
        self.ids
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push(id);
    }

    /// Takes everything recorded so far.
    fn take(&self) -> Vec<SheetId> {
        core::mem::take(&mut self.ids.lock().unwrap_or_else(|held| held.into_inner()))
    }
}

/// One installed sheet.
struct Installed {
    /// Its identifier.
    id: SheetId,
    /// The origin it cascades at.
    origin: SheetOrigin,
    /// The parsed sheet the rule set holds.
    sheet: DocumentStyleSheet,
}

/// Every sheet installed in one rule set, in cascade order within each origin.
pub(crate) struct SheetSet {
    /// The sheets, in installation order.
    installed: Vec<Installed>,
    /// The next identifier to hand out.
    next: u64,
    /// Handles that have been dropped and not yet acted on.
    dropped: Arc<DroppedSheets>,
}

impl SheetSet {
    /// A set with no sheets in it.
    pub(crate) fn new() -> Self {
        Self {
            installed: Vec::new(),
            next: 0,
            dropped: Arc::new(DroppedSheets::default()),
        }
    }

    /// Adds `sheet` at the end of `origin`'s sheets.
    pub(crate) fn append(
        &mut self,
        stylist: &mut Stylist,
        guard: &SharedRwLockReadGuard,
        origin: SheetOrigin,
        sheet: DocumentStyleSheet,
    ) -> SheetHandle {
        stylist.append_stylesheet(sheet.clone(), guard);
        self.record(origin, sheet)
    }

    /// Adds `sheet` immediately before the sheet `before` names.
    ///
    /// # Panics
    ///
    /// Panics if `before` names no installed sheet, which can only happen if it names a sheet of a
    /// different rule set.
    pub(crate) fn insert_before(
        &mut self,
        stylist: &mut Stylist,
        guard: &SharedRwLockReadGuard,
        origin: SheetOrigin,
        sheet: DocumentStyleSheet,
        before: &SheetHandle,
    ) -> SheetHandle {
        let position = self
            .position_of(before.id())
            .expect("the handle names a sheet installed in this rule set");
        let existing = self.installed[position].sheet.clone();
        stylist.insert_stylesheet_before(sheet.clone(), existing, guard);
        let handle = self.record(origin, sheet);
        // Keep our own order the same as the rule set's, so that a later replacement finds the
        // right neighbour to re-insert in front of.
        let inserted = self.installed.pop().expect("the sheet was just recorded");
        self.installed.insert(position, inserted);
        handle
    }

    /// Replaces the sheet `handle` names with `sheet`, keeping its cascade position.
    ///
    /// # Panics
    ///
    /// Panics if `handle` names no installed sheet.
    pub(crate) fn replace(
        &mut self,
        stylist: &mut Stylist,
        guard: &SharedRwLockReadGuard,
        handle: &SheetHandle,
        sheet: DocumentStyleSheet,
    ) {
        let position = self
            .position_of(handle.id())
            .expect("the handle names a sheet installed in this rule set");
        let origin = self.installed[position].origin;
        let old = self.installed[position].sheet.clone();
        // The next sheet at the same origin is what decides the position: the rule set orders
        // within an origin, so re-inserting in front of the same neighbour restores the place the
        // sheet had rather than moving it to the end.
        let next = self.installed[position + 1..]
            .iter()
            .find(|entry| entry.origin == origin)
            .map(|entry| entry.sheet.clone());
        stylist.remove_stylesheet(old, guard);
        match next {
            Some(next) => stylist.insert_stylesheet_before(sheet.clone(), next, guard),
            None => stylist.append_stylesheet(sheet.clone(), guard),
        }
        self.installed[position].sheet = sheet;
    }

    /// Removes every sheet whose handle has been dropped, and reports how many were removed.
    pub(crate) fn remove_dropped(
        &mut self,
        stylist: &mut Stylist,
        guard: &SharedRwLockReadGuard,
    ) -> usize {
        let dropped = self.dropped.take();
        for id in &dropped {
            let Some(position) = self.position_of(*id) else {
                continue;
            };
            let entry = self.installed.remove(position);
            stylist.remove_stylesheet(entry.sheet, guard);
        }
        dropped.len()
    }

    /// Whether any handle has been dropped and not yet acted on.
    pub(crate) fn has_dropped(&self) -> bool {
        !self
            .dropped
            .ids
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .is_empty()
    }

    /// Records a sheet the rule set has already been given.
    fn record(&mut self, origin: SheetOrigin, sheet: DocumentStyleSheet) -> SheetHandle {
        let id = SheetId(self.next);
        self.next += 1;
        self.installed.push(Installed { id, origin, sheet });
        SheetHandle {
            id,
            dropped: Arc::clone(&self.dropped),
        }
    }

    /// The origin the sheet `id` names cascades at.
    pub(crate) fn origin_of(&self, id: SheetId) -> Option<SheetOrigin> {
        self.position_of(id).map(|at| self.installed[at].origin)
    }

    /// Where the sheet `id` names sits in installation order.
    fn position_of(&self, id: SheetId) -> Option<usize> {
        self.installed.iter().position(|entry| entry.id == id)
    }
}
