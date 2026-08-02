//! Unit and property tests for [`IntervalSet`](super::IntervalSet).

use std::collections::BTreeSet;
use std::ops::Range;

use proptest::prelude::*;

use super::IntervalSet;

/// Every byte covered by `ranges`, computed the slow and obvious way.
fn covered(ranges: &[Range<u64>]) -> BTreeSet<u64> {
    ranges.iter().flat_map(|range| range.clone()).collect()
}

/// Small ranges, so the byte-set oracle above stays cheap.
fn any_range() -> impl Strategy<Value = Range<u64>> {
    (0u64..64, 0u64..8).prop_map(|(start, len)| start..start + len)
}

proptest! {
    /// The set covers exactly the bytes it was given, no more and no fewer.
    #[test]
    fn the_set_covers_exactly_what_it_was_given(ranges in prop::collection::vec(any_range(), 0..24)) {
        let set: IntervalSet = ranges.iter().cloned().collect();
        let spans: Vec<Range<u64>> = set.spans().collect();
        prop_assert_eq!(covered(&spans), covered(&ranges));
        prop_assert_eq!(set.total_len(), covered(&ranges).len() as u64);
    }

    /// The spans stay sorted, and no two of them overlap or even touch: a set holding two
    /// adjacent spans would describe two buffer copies where one would do.
    #[test]
    fn spans_stay_sorted_disjoint_and_non_adjacent(ranges in prop::collection::vec(any_range(), 0..24)) {
        let set: IntervalSet = ranges.into_iter().collect();
        let spans: Vec<Range<u64>> = set.spans().collect();
        for span in &spans {
            prop_assert!(span.start < span.end);
        }
        for pair in spans.windows(2) {
            prop_assert!(pair[0].end < pair[1].start);
        }
    }

    /// Insertion order cannot change the result.
    #[test]
    fn insertion_order_does_not_matter(ranges in prop::collection::vec(any_range(), 0..16)) {
        let forwards: IntervalSet = ranges.iter().cloned().collect();
        let backwards: IntervalSet = ranges.iter().rev().cloned().collect();
        prop_assert_eq!(forwards, backwards);
    }

    /// Membership agrees with the byte set, at every offset in range and just past both ends.
    #[test]
    fn membership_agrees_with_the_oracle(ranges in prop::collection::vec(any_range(), 0..16)) {
        let bytes = covered(&ranges);
        let set: IntervalSet = ranges.into_iter().collect();
        for offset in 0..72u64 {
            prop_assert_eq!(set.contains(offset), bytes.contains(&offset));
        }
    }

    /// A union is the same as inserting the other set's ranges one at a time.
    #[test]
    fn union_matches_repeated_insertion(
        left in prop::collection::vec(any_range(), 0..12),
        right in prop::collection::vec(any_range(), 0..12),
    ) {
        let mut merged: IntervalSet = left.iter().cloned().collect();
        merged.union(&right.iter().cloned().collect());
        let combined: IntervalSet = left.into_iter().chain(right).collect();
        prop_assert_eq!(merged, combined);
    }
}

#[test]
fn an_empty_range_covers_nothing() {
    let mut set = IntervalSet::new();
    set.insert(8..8);
    set.insert(Range { start: 9, end: 4 });
    assert!(set.is_empty());
    assert_eq!(set.bounds(), None);
    assert_eq!(set.total_len(), 0);
}

#[test]
fn a_range_bridging_two_spans_collapses_all_three() {
    let mut set: IntervalSet = [0..4, 8..12, 16..20].into_iter().collect();
    assert_eq!(set.len(), 3);
    set.insert(3..17);
    assert_eq!(set.len(), 1);
    assert_eq!(set.bounds(), Some(0..20));
}

#[test]
fn touching_ranges_coalesce_but_a_one_byte_gap_does_not() {
    let mut touching: IntervalSet = [0..4, 4..8].into_iter().collect();
    assert_eq!(touching.len(), 1);
    assert_eq!(touching.bounds(), Some(0..8));
    touching.insert(9..12);
    assert_eq!(touching.spans().collect::<Vec<_>>(), [0..8, 9..12]);
}

#[test]
fn intersection_is_half_open_at_both_ends() {
    let set: IntervalSet = std::iter::once(4..8).collect();
    assert!(set.intersects(&(7..9)));
    assert!(!set.intersects(&(8..9)));
    assert!(!set.intersects(&(0..4)));
    assert!(!set.intersects(&(4..4)));
}

#[test]
fn bounds_span_the_whole_set() {
    let set: IntervalSet = [16..20, 0..4].into_iter().collect();
    assert_eq!(set.bounds(), Some(0..20));
}

#[test]
fn clearing_empties_the_set() {
    let mut set: IntervalSet = std::iter::once(0..4).collect();
    set.clear();
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
}
