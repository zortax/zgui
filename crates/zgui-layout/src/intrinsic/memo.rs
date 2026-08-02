//! What each box measured at its narrowest and its widest.

use rustc_hash::FxHashMap;
use zgui_dom::side::BoxKey;

use crate::axis::Axis;
use crate::style::convert::length::IntrinsicSizes;

/// Intrinsic measurements, held for one layout pass.
///
/// Keyed by the box and the axis, and taken against one containing block: a measurement is only
/// valid for the inline size it was taken at, because a percentage inside the content resolves
/// against it. The whole memo is therefore discarded when the containing block changes, rather than
/// entries being invalidated one at a time.
#[derive(Debug, Default)]
pub struct IntrinsicMemo {
    /// What each box measured on each axis.
    entries: FxHashMap<(BoxKey, Axis), IntrinsicSizes>,
    /// How many measurements were taken.
    measurements: u32,
}

impl IntrinsicMemo {
    /// What one box measured on one axis, if it was measured.
    pub fn get(&self, key: BoxKey, axis: Axis) -> Option<IntrinsicSizes> {
        self.entries.get(&(key, axis)).copied()
    }

    /// Records one measurement.
    pub fn insert(&mut self, key: BoxKey, axis: Axis, sizes: IntrinsicSizes) {
        if self.entries.insert((key, axis), sizes).is_none() {
            self.measurements += 1;
        }
    }

    /// How many boxes-and-axes were measured.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing was measured, which is the case for every document that writes no sizing
    /// keyword.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many measurements were taken, counting each box-and-axis once.
    pub fn measurements(&self) -> u32 {
        self.measurements
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.measurements = 0;
    }
}

#[cfg(test)]
mod tests {
    use zgui_arena::{DomainId, Generation};
    use zgui_dom::side::BoxKey;

    use crate::axis::Axis;
    use crate::style::convert::length::IntrinsicSizes;

    use super::IntrinsicMemo;

    fn key(index: u32) -> BoxKey {
        BoxKey::new(index, Generation::FIRST, DomainId::FIRST)
    }

    #[test]
    fn the_two_axes_of_one_box_are_measured_independently() {
        let mut memo = IntrinsicMemo::default();
        let sizes = IntrinsicSizes {
            min: 10.0,
            max: 90.0,
        };
        memo.insert(key(1), Axis::Horizontal, sizes);
        assert_eq!(memo.get(key(1), Axis::Horizontal), Some(sizes));
        assert_eq!(memo.get(key(1), Axis::Vertical), None);
        assert_eq!(memo.measurements(), 1);
    }

    #[test]
    fn re_recording_one_entry_does_not_count_a_second_measurement() {
        let mut memo = IntrinsicMemo::default();
        let sizes = IntrinsicSizes { min: 1.0, max: 2.0 };
        memo.insert(key(1), Axis::Horizontal, sizes);
        memo.insert(
            key(1),
            Axis::Horizontal,
            IntrinsicSizes { min: 3.0, max: 4.0 },
        );
        assert_eq!(memo.measurements(), 1);
        assert_eq!(memo.len(), 1);
    }
}
