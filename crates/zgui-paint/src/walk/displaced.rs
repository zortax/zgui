//! Chunks whose fragments layout moved while the walk skipped them.
//!
//! Such a chunk would stay drawn at the old place, which none of the boxes the walk tests covers
//! any more. It is withdrawn instead, and the fragment is recorded again once the walk reaches it.

use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_layout::{Fragment, LayoutStore};

use crate::sync::CompositorSync;
use crate::walk::replay::PaintCache;

/// Withdraws the chunk of `fragment` if the fragment moved away from it.
pub(super) fn withdraw_if_moved(
    fragment: &Fragment,
    cache: &PaintCache,
    sync: &mut CompositorSync,
) {
    if sync.knows(fragment.key) && cache.moved_from(fragment).is_some() {
        sync.withdraw(fragment.key);
    }
}

/// Withdraws the chunks of the fragments under `key` that moved away from them, `key`'s own
/// included.
///
/// A box below `key` whose subtree is laid out where it was when the walk last reached it is
/// skipped with everything under it.
pub(super) fn sweep(
    store: &LayoutStore,
    key: BoxKey,
    cache: &PaintCache,
    sync: &mut CompositorSync,
) {
    let mut stack = vec![key];
    while let Some(key) = stack.pop() {
        for frag in store.fragments_of_box(key) {
            if let Some(fragment) = store.fragment(*frag) {
                withdraw_if_moved(fragment, cache, sync);
            }
        }
        let Some(node) = store.get(key) else {
            continue;
        };
        for &child in &node.children {
            if let Some((first, ink)) = laid_out(store, child)
                && sync.subtree_moved(first, ink)
            {
                stack.push(child);
            }
        }
    }
}

/// Returns the first fragment of `key` and the union of its fragments' subtree inks, each in its
/// own anchor, or `None` for a box without fragments.
pub(super) fn laid_out(
    store: &LayoutStore,
    key: BoxKey,
) -> Option<(zgui_layout::FragKey, Rect<DevicePx, Device>)> {
    let fragments = store.fragments_of_box(key);
    let first = *fragments.first()?;
    let ink = fragments
        .iter()
        .filter_map(|frag| store.fragment(*frag))
        .map(|fragment| fragment.subtree_ink)
        .reduce(|union, ink| union.union(ink))?;
    Some((first, ink))
}
