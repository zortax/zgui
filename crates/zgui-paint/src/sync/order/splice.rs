//! The splices that turn one list of order entries into another.

use compositor_types::{OrderEntry, OrderId, Transaction};
use rustc_hash::{FxHashMap, FxHashSet};

/// Replaces `old` with `new` as the list of `node`, and returns the number of entries removed and
/// inserted.
///
/// The entries both lists hold in the same relative order stay where they are, as many of them as
/// possible, and every run of entries between two of them is written as one splice. A list with an
/// entry twice is written as one splice of the part between the common prefix and suffix.
pub(super) fn splice(
    tx: &mut Transaction,
    node: OrderId,
    old: &[OrderEntry],
    new: &[OrderEntry],
) -> usize {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old = &old[prefix..old.len() - suffix];
    let new = &new[prefix..new.len() - suffix];
    if old.is_empty() && new.is_empty() {
        return 0;
    }
    if let Some(spliced) = shifted(tx, node, prefix as u32, old, new) {
        return spliced;
    }

    let kept = kept(old, new).unwrap_or_default();

    let mut at = prefix as u32;
    let mut from = [0, 0];
    let mut spliced = 0;
    for [to_old, to_new] in kept.into_iter().chain([[old.len(), new.len()]]) {
        let removed = to_old - from[0];
        let inserted = &new[from[1]..to_new];
        if removed > 0 || !inserted.is_empty() {
            tx.splice(node, at, removed as u32, inserted);
        }
        spliced += removed + inserted.len();
        at += inserted.len() as u32 + 1;
        from = [to_old + 1, to_new + 1];
    }
    spliced
}

/// Writes `new` over `old` at `at` when `new` drops entries at one end of `old` and adds entries at
/// the other, and returns the number of entries removed and inserted.
///
/// Returns `None` and writes nothing for any other change.
fn shifted(
    tx: &mut Transaction,
    node: OrderId,
    at: u32,
    old: &[OrderEntry],
    new: &[OrderEntry],
) -> Option<usize> {
    if old.is_empty() || new.is_empty() {
        tx.splice(node, at, old.len() as u32, new);
        return Some(old.len() + new.len());
    }
    // The entries left at the front, then the entries added at the back.
    if let Some(dropped) = new
        .first()
        .and_then(|first| old.iter().position(|e| e == first))
    {
        let kept = old.len() - dropped;
        if new.len() >= kept && old[dropped..] == new[..kept] {
            if dropped > 0 {
                tx.splice(node, at, dropped as u32, &[]);
            }
            if new.len() > kept {
                tx.splice(node, at + kept as u32, 0, &new[kept..]);
            }
            return Some(dropped + new.len() - kept);
        }
    }
    // The entries added at the front, then the entries left at the back.
    if let Some(added) = old
        .first()
        .and_then(|first| new.iter().position(|e| e == first))
    {
        let kept = new.len() - added;
        if old.len() >= kept && new[added..] == old[..kept] {
            if added > 0 {
                tx.splice(node, at, 0, &new[..added]);
            }
            if old.len() > kept {
                tx.splice(node, at + new.len() as u32, (old.len() - kept) as u32, &[]);
            }
            return Some(added + old.len() - kept);
        }
    }
    None
}

/// Returns the positions in `old` and in `new` of the most entries both hold in the same order,
/// in increasing order, or `None` when either holds an entry twice.
fn kept(old: &[OrderEntry], new: &[OrderEntry]) -> Option<Vec<[usize; 2]>> {
    let mut place = FxHashMap::default();
    for (index, entry) in old.iter().enumerate() {
        if place.insert(*entry, index).is_some() {
            return None;
        }
    }
    let mut seen = FxHashSet::default();
    let mut pairs = Vec::new();
    for (index, entry) in new.iter().enumerate() {
        if !seen.insert(*entry) {
            return None;
        }
        if let Some(&at) = place.get(entry) {
            pairs.push([at, index]);
        }
    }
    Some(increasing(&pairs))
}

/// Returns the longest subsequence of `pairs`, which ascend in their second position, that also
/// ascends in its first.
fn increasing(pairs: &[[usize; 2]]) -> Vec<[usize; 2]> {
    // Patience sorting: `tails[k]` is the pair ending the best run of length `k + 1` found so far,
    // and `before[i]` the pair ahead of `pairs[i]` in its run.
    let mut tails: Vec<usize> = Vec::new();
    let mut before = vec![usize::MAX; pairs.len()];
    for (index, pair) in pairs.iter().enumerate() {
        let length = tails.partition_point(|&tail| pairs[tail][0] < pair[0]);
        if length > 0 {
            before[index] = tails[length - 1];
        }
        if length == tails.len() {
            tails.push(index);
        } else {
            tails[length] = index;
        }
    }
    let mut run = Vec::with_capacity(tails.len());
    let mut next = tails.last().copied().unwrap_or(usize::MAX);
    while next != usize::MAX {
        run.push(pairs[next]);
        next = before[next];
    }
    run.reverse();
    run
}
