//! A sorted set of disjoint byte ranges.

mod insert;
#[cfg(test)]
mod tests;

use core::ops::Range;

/// A set of byte ranges, kept sorted, disjoint and non-adjacent.
///
/// Adding a range that overlaps or merely touches one already in the set merges the two, so the
/// set always holds the smallest number of ranges that covers the same bytes. That is what makes
/// it useful for deciding what to copy into a buffer: iterating the set yields one contiguous
/// span per copy, never two spans that could have been one.
///
/// Ranges are half-open, and an empty range covers nothing and is ignored.
///
/// ```
/// use zgui_bits::IntervalSet;
///
/// let mut dirty = IntervalSet::new();
/// dirty.insert(0..16);
/// dirty.insert(32..48);
/// assert_eq!(dirty.spans().collect::<Vec<_>>(), [0..16, 32..48]);
///
/// // Touching ranges coalesce, so the set never asks for two copies where one would do.
/// dirty.insert(16..32);
/// assert_eq!(dirty.spans().collect::<Vec<_>>(), [0..48]);
/// assert_eq!(dirty.total_len(), 48);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IntervalSet {
    /// The covered ranges, sorted by start, pairwise disjoint and never adjacent.
    spans: Vec<Range<u64>>,
}

impl IntervalSet {
    /// An empty set.
    pub const fn new() -> Self {
        Self { spans: Vec::new() }
    }

    /// An empty set with room for `capacity` ranges before it reallocates.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            spans: Vec::with_capacity(capacity),
        }
    }

    /// Adds `range`, merging it with every range it overlaps or touches.
    ///
    /// An empty or reversed range is ignored.
    pub fn insert(&mut self, range: Range<u64>) {
        insert::insert(&mut self.spans, range);
    }

    /// Adds every range of `other`.
    pub fn union(&mut self, other: &Self) {
        for span in &other.spans {
            self.insert(span.clone());
        }
    }

    /// Removes every range, keeping the allocation for the next frame.
    pub fn clear(&mut self) {
        self.spans.clear();
    }

    /// Whether the set covers no bytes at all.
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// How many disjoint ranges the set holds, which is how many copies it describes.
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// The ranges, in ascending order.
    pub fn spans(&self) -> impl ExactSizeIterator<Item = Range<u64>> + '_ {
        self.spans.iter().cloned()
    }

    /// The total number of bytes covered.
    pub fn total_len(&self) -> u64 {
        self.spans.iter().map(|span| span.end - span.start).sum()
    }

    /// The smallest range containing every range in the set, or `None` when it is empty.
    pub fn bounds(&self) -> Option<Range<u64>> {
        match (self.spans.first(), self.spans.last()) {
            (Some(first), Some(last)) => Some(first.start..last.end),
            _ => None,
        }
    }

    /// Whether `offset` lies in one of the ranges.
    pub fn contains(&self, offset: u64) -> bool {
        self.spans
            .binary_search_by(|span| {
                if span.end <= offset {
                    core::cmp::Ordering::Less
                } else if span.start > offset {
                    core::cmp::Ordering::Greater
                } else {
                    core::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }

    /// Whether any range in the set shares a byte with `range`.
    ///
    /// An empty `range` shares nothing with anything.
    pub fn intersects(&self, range: &Range<u64>) -> bool {
        range.start < range.end
            && self
                .spans
                .iter()
                .any(|span| span.start < range.end && range.start < span.end)
    }
}

impl FromIterator<Range<u64>> for IntervalSet {
    fn from_iter<I: IntoIterator<Item = Range<u64>>>(ranges: I) -> Self {
        let mut set = Self::new();
        for range in ranges {
            set.insert(range);
        }
        set
    }
}
