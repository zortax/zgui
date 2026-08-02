//! The coalescing insert that keeps a span list sorted, disjoint and non-adjacent.

use core::ops::Range;

/// Inserts `range` into `spans`, merging it with every span it overlaps or touches.
///
/// `spans` is sorted by start, pairwise disjoint and never adjacent on entry, and has all three
/// properties again on exit. An empty or reversed `range` is ignored.
pub(super) fn insert(spans: &mut Vec<Range<u64>>, range: Range<u64>) {
    if range.start >= range.end {
        return;
    }

    // The first span that could touch `range` is the first whose end is not below its start;
    // `end < range.start` — rather than `<=` — is what makes an exactly adjacent span merge.
    let first = spans.partition_point(|span| span.end < range.start);
    // The first span strictly beyond it, by the same reasoning at the far end.
    let last = spans.partition_point(|span| span.start <= range.end);

    if first == last {
        spans.insert(first, range);
        return;
    }

    let start = range.start.min(spans[first].start);
    let end = range.end.max(spans[last - 1].end);
    spans.splice(first..last, core::iter::once(start..end));
}
