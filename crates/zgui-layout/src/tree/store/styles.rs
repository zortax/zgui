//! The distinct computed styles the document's boxes hold, interned once each.
//!
//! Many boxes share one cascade allocation — every row of a list, every item of a menu — so the
//! store keeps one entry per distinct allocation and each box records which entry it holds. The
//! entry is where per-style derivations will live: a lowering is computed once per entry rather
//! than once per box, and dropped when the last box holding the entry lets go.

use rustc_hash::FxHashMap;
use zgui_css::ComputedStyle;
use zgui_profile::{Counter, counter};

use crate::style::DeviceStyle;
use crate::style::calc::CalcTable;
use crate::style::lowered::LayoutStyle;

/// One document-local name for a distinct computed style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StyleSlot(u32);

/// One distinct computed style and everything derived from it.
#[derive(Debug)]
struct StyleEntry {
    /// The style itself.
    ///
    /// A strong clone: it keeps the allocation alive, so the address this entry is indexed under
    /// cannot be reused by a different cascade result while the entry exists.
    style: ComputedStyle,
    /// How many boxes hold this entry.
    refs: u32,
    /// The style in the layout algorithms' vocabulary, once a pass has asked for it.
    lowered: Option<LayoutStyle>,
    /// The `calc()` identifiers the lowering interned, given back with it.
    calc_ids: Vec<u32>,
}

/// Every distinct computed style held by a live box.
#[derive(Debug, Default)]
pub(crate) struct StyleTable {
    /// The entries, indexed by slot.
    entries: Vec<Option<StyleEntry>>,
    /// Slots whose entry was dropped and may be reissued.
    free: Vec<u32>,
    /// Cascade allocation address to its slot.
    by_cascade: FxHashMap<usize, u32>,
    /// Slots waiting to be lowered.
    pending: Vec<u32>,
    /// The device every held lowering was lowered for.
    lowered_for: Option<DeviceStyle>,
    /// The `calc()` expressions the lowerings refer to.
    calc: CalcTable,
}

/// The address a cascade result is interned under.
///
/// Allocation identity, exactly as [`same_cascade`](crate::style::same_cascade) compares it.
fn cascade_address(style: &ComputedStyle) -> usize {
    ::core::ptr::from_ref(&**style) as usize
}

impl StyleTable {
    /// The slot for `style`, interning it if no live box holds it yet.
    pub(crate) fn intern(&mut self, style: &ComputedStyle) -> StyleSlot {
        let address = cascade_address(style);
        if let Some(&index) = self.by_cascade.get(&address) {
            let entry = self.entries[index as usize]
                .as_mut()
                .expect("an indexed slot holds an entry");
            entry.refs += 1;
            return StyleSlot(index);
        }
        let entry = StyleEntry {
            style: style.clone(),
            refs: 1,
            lowered: None,
            calc_ids: Vec::new(),
        };
        let index = match self.free.pop() {
            Some(index) => {
                self.entries[index as usize] = Some(entry);
                index
            }
            None => {
                let index = u32::try_from(self.entries.len()).expect("fewer than 2^32 styles");
                self.entries.push(Some(entry));
                index
            }
        };
        self.by_cascade.insert(address, index);
        self.pending.push(index);
        StyleSlot(index)
    }

    /// Lets go of one box's hold on a slot, dropping the entry with the last hold.
    pub(crate) fn release(&mut self, slot: StyleSlot) {
        let index = slot.0 as usize;
        let Some(entry) = self.entries[index].as_mut() else {
            debug_assert!(false, "released a slot twice");
            return;
        };
        entry.refs -= 1;
        if entry.refs > 0 {
            return;
        }
        let entry = self.entries[index].take().expect("the entry checked above");
        self.by_cascade.remove(&cascade_address(&entry.style));
        for id in entry.calc_ids {
            self.calc.release(id);
        }
        self.free.push(slot.0);
    }

    /// Lowers every style that owes a lowering for `device`.
    ///
    /// Called once at the head of a layout pass, after every style write of the frame and before
    /// any read. A device change owes every entry a fresh lowering, because absolute lengths are
    /// scaled at lowering time; that rides the full relayout the change already forces.
    pub(crate) fn ensure_lowered(&mut self, device: DeviceStyle) {
        if self.lowered_for != Some(device) {
            self.calc.set_scale(device.scale);
            self.pending.clear();
            for (index, entry) in self.entries.iter_mut().enumerate() {
                let Some(entry) = entry.as_mut() else {
                    continue;
                };
                entry.lowered = None;
                for id in entry.calc_ids.drain(..) {
                    self.calc.release(id);
                }
                self.pending.push(index as u32);
            }
            self.lowered_for = Some(device);
        }
        while let Some(index) = self.pending.pop() {
            // A pending slot may have died, or died and been reissued to a style that was then
            // lowered through its own pending entry. Both read as "nothing owed here".
            let Some(entry) = self.entries[index as usize].as_mut() else {
                continue;
            };
            if entry.lowered.is_some() {
                continue;
            }
            entry.lowered = Some(LayoutStyle::lower(&entry.style, device, &mut self.calc));
            self.calc.drain_issued(&mut entry.calc_ids);
            counter::bump(Counter::StylesLowered);
        }
    }

    /// The lowering a slot holds.
    ///
    /// # Panics
    ///
    /// If the slot holds no entry, or [`StyleTable::ensure_lowered`] has not run since the entry
    /// was interned.
    pub(crate) fn lowered(&self, slot: StyleSlot) -> &LayoutStyle {
        self.entries[slot.0 as usize]
            .as_ref()
            .expect("a live style slot")
            .lowered
            .as_ref()
            .expect("a pass lowers every style before reading any")
    }

    /// Resolves a `calc()` handle a lowering embedded.
    pub(crate) fn resolve_calc(&self, value: *const (), basis: f32) -> f32 {
        self.calc.resolve(value, basis)
    }

    /// How many `calc()` expressions the lowerings hold.
    pub(crate) fn interned_calcs(&self) -> usize {
        self.calc.live()
    }

    /// The style a slot names.
    ///
    /// # Panics
    ///
    /// If the slot holds no entry.
    pub(crate) fn style(&self, slot: StyleSlot) -> &ComputedStyle {
        &self.entries[slot.0 as usize]
            .as_ref()
            .expect("a live style slot")
            .style
    }

    /// How many distinct styles are live.
    pub(crate) fn live(&self) -> usize {
        self.entries.iter().filter(|it| it.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;

    use super::*;

    #[test]
    fn one_allocation_is_one_entry_however_many_boxes_hold_it() {
        let mut table = StyleTable::default();
        let style = StyleDraft::initial().build();
        let first = table.intern(&style);
        let second = table.intern(&style);
        assert_eq!(first, second);
        assert_eq!(table.live(), 1);
        table.release(first);
        assert_eq!(table.live(), 1, "one box still holds the entry");
        table.release(second);
        assert_eq!(table.live(), 0);
    }

    #[test]
    fn distinct_allocations_are_distinct_entries() {
        let mut table = StyleTable::default();
        let first_style = StyleDraft::initial().build();
        let second_style = StyleDraft::initial().build();
        let first = table.intern(&first_style);
        let second = table.intern(&second_style);
        assert_ne!(first, second, "two cascade results never share an entry");
        assert_eq!(table.live(), 2);
    }

    #[test]
    fn a_dropped_entry_leaves_its_slot_reusable_without_aliasing() {
        let mut table = StyleTable::default();
        let first_style = StyleDraft::initial().build();
        let slot = table.intern(&first_style);
        table.release(slot);
        let second_style = StyleDraft::initial().build();
        let reused = table.intern(&second_style);
        assert_eq!(slot, reused, "a dead slot grows the table forever");
        assert!(crate::style::same_cascade(
            table.style(reused),
            &second_style
        ));
    }

    #[test]
    fn lowering_serves_every_interned_style_and_survives_reinterning() {
        let mut table = StyleTable::default();
        let style = StyleDraft::initial().build();
        let slot = table.intern(&style);
        table.ensure_lowered(DeviceStyle::default());
        let _ = table.lowered(slot);

        // A style interned after the pass began is owed a lowering by the next pass.
        let late_style = StyleDraft::initial().build();
        let late = table.intern(&late_style);
        table.ensure_lowered(DeviceStyle::default());
        let _ = table.lowered(late);
    }

    #[test]
    fn a_device_change_owes_every_entry_a_fresh_lowering() {
        let mut table = StyleTable::default();
        let mut draft = StyleDraft::initial();
        draft.position_group().width = zgui_css::values::size::SizeValue::LengthPercentage(
            zgui_css::values::length::NonNegative(
                zgui_css::values::length::LengthPercentage::new_length(
                    zgui_css::values::length::Length::new(10.0),
                ),
            ),
        );
        let style = draft.build();
        let slot = table.intern(&style);

        table.ensure_lowered(DeviceStyle {
            scale: 1.0,
            scrollbar_width: 15.0,
        });
        assert_eq!(
            table.lowered(slot).size.width,
            taffy::Dimension::length(10.0)
        );

        table.ensure_lowered(DeviceStyle {
            scale: 2.0,
            scrollbar_width: 15.0,
        });
        assert_eq!(
            table.lowered(slot).size.width,
            taffy::Dimension::length(20.0),
            "absolute lengths are scaled at lowering time"
        );
    }

    #[test]
    fn a_slot_that_died_while_pending_is_skipped() {
        let mut table = StyleTable::default();
        let style = StyleDraft::initial().build();
        let slot = table.intern(&style);
        table.release(slot);
        table.ensure_lowered(DeviceStyle::default());
        assert_eq!(table.live(), 0);
    }
}
