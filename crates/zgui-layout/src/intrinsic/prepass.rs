//! Measuring the boxes whose sizes are written as content keywords, before laying anything out.
//!
//! The pass visits only the boxes that need it, and finds them by asking the store's roster of
//! content-keyword boxes rather than by walking the document. In a document that writes no content
//! keyword, which is the overwhelming majority, that is one test against an empty list and nothing
//! else at all.
//!
//! # Deepest first
//!
//! A box written `max-content` inside a box written `max-content` has to be measured *before* the
//! box that contains it: until its own keyword has an answer it reads as `auto`, and a container
//! measured while its child says `auto` is measured against a child that will not be that size.
//! So the boxes that need measuring are visited deepest first, and every answer is in place before
//! anything above it is asked.
//!
//! The order used to come free from reversing a pre-order walk. A roster carries no order, so the
//! depth is computed — over the boxes actually being measured, which is normally none of them and
//! never more than the ones whose caches something invalidated.
//!
//! # What is measured, and what is held
//!
//! An answer already held is not taken again. That is the whole saving: an intrinsic measurement is
//! a full nested layout of the box's subtree, taken twice per axis, and a `width: fit-content`
//! button whose contents did not change wants the same answer this frame as last. The store's
//! `BoxLayout::intrinsic` states why holding it is sound and what empties it.
//!
//! Taking an answer does invalidate the two *cached-size* storeys of the box it belongs to. The
//! probes that produced it ran while the keyword still read as `auto`, and an entry cached under
//! that reading would otherwise be handed back during the real layout, when the keyword means a
//! length. The answer just computed is deliberately kept, which is why this alone among the
//! invalidators calls `BoxLayout::forget_cached_sizes`.

use rustc_hash::FxHashMap;
use taffy::{
    AvailableSpace, LayoutInput, LayoutPartialTree, RequestedAxis, RunMode, Size, SizingMode,
};
use zgui_dom::side::BoxKey;

use crate::axis::Axis;
use crate::key::to_node_id;
use crate::measure::MeasureContent;
use crate::style::convert::length::IntrinsicSizes;
use crate::tree::LayoutTree;

/// Measures every box under `root` that is sized by a content keyword and is not already holding
/// the answer.
pub fn run<C: MeasureContent>(tree: &mut LayoutTree<'_, C>, root: BoxKey) {
    if tree.store().no_content_keywords() {
        return;
    }
    let mut roster = tree.store_mut().take_content_roster();
    let mut needed: Vec<(BoxKey, [bool; 2], u32)> = Vec::new();
    let mut depths: FxHashMap<BoxKey, u32> = FxHashMap::default();
    roster.entries.retain(|&key| {
        if !tree.store().contains(key) {
            return false;
        }
        let axes = tree.store().content_axes(key);
        if axes == [false, false] {
            return false;
        }
        // Only the axes still without an answer. A box holding one on each axis stays on the
        // roster — it is still a content-keyword box — and is simply not measured.
        let owed = [
            axes[0] && tree.store().intrinsic(key, Axis::Horizontal).is_none(),
            axes[1] && tree.store().intrinsic(key, Axis::Vertical).is_none(),
        ];
        if owed[0] || owed[1] {
            let depth = depth_of(tree, key, &mut depths);
            needed.push((key, owed, depth));
        }
        true
    });
    tree.store_mut().restore_content_roster(roster);
    // Deepest first, and by key within a depth so that two documents built the same way measure in
    // the same order — a golden that depends on the order would otherwise depend on the hash seed.
    needed.sort_unstable_by(|a, b| b.2.cmp(&a.2).then(a.0.index().cmp(&b.0.index())));
    debug_assert!(
        needed.iter().all(|&(key, _, _)| reaches(tree, key, root)),
        "the pre-pass is measuring a box that hangs off no tree, so nothing will ever invalidate \
         it and the measurement will be taken again on every frame for ever"
    );
    for (key, axes, _) in needed {
        for axis in Axis::BOTH {
            if axes[axis.index()] {
                let sizes = measure(tree, key, axis);
                tree.store_mut().set_intrinsic(key, axis, sizes);
            }
        }
        tree.store_mut().state_mut(key).forget_cached_sizes();
    }
}

/// Whether `key` still hangs off `root`.
///
/// The walk this pass replaced started at the root, so it could only ever reach boxes attached to
/// the tree; the roster holds every box that was registered, attached or not. Removing a box takes
/// it off the roster, so the two agree — and this is what says so, in debug builds, rather than the
/// agreement being assumed. A detached box is not a wrong answer, it is a measurement taken on
/// every pass for a box no layout will ever ask about, which is invisible in every output.
fn reaches<C>(tree: &LayoutTree<'_, C>, key: BoxKey, root: BoxKey) -> bool {
    let mut at = Some(key);
    while let Some(current) = at {
        if current == root {
            return true;
        }
        at = tree.store().get(current).and_then(|node| node.parent);
    }
    false
}

/// How far below the root a box sits, memoised across the batch.
///
/// The memo is what keeps this linear in the document's depth rather than quadratic in it when a
/// run of keyword boxes shares an ancestor chain, which is the shape a list of chips has.
fn depth_of<C>(tree: &LayoutTree<'_, C>, key: BoxKey, memo: &mut FxHashMap<BoxKey, u32>) -> u32 {
    let mut chain = Vec::new();
    let mut current = Some(key);
    let mut depth = 0;
    while let Some(at) = current {
        if let Some(&known) = memo.get(&at) {
            depth = known;
            break;
        }
        let Some(node) = tree.store().get(at) else {
            break;
        };
        chain.push(at);
        current = node.parent;
    }
    for at in chain.into_iter().rev() {
        depth += 1;
        memo.insert(at, depth);
    }
    depth
}

/// The narrowest and widest one box can be on one axis.
fn measure<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    axis: Axis,
) -> IntrinsicSizes {
    IntrinsicSizes {
        min: probe(tree, key, axis, AvailableSpace::MinContent),
        max: probe(tree, key, axis, AvailableSpace::MaxContent),
    }
}

/// One probe, taken with the box's own sizing styles ignored.
///
/// They have to be ignored: the probe exists to answer what the *content* wants, and the style that
/// asked the question is the one being resolved.
fn probe<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    axis: Axis,
    space: AvailableSpace,
) -> f32 {
    let available = match axis {
        Axis::Horizontal => Size {
            width: space,
            height: AvailableSpace::MaxContent,
        },
        Axis::Vertical => Size {
            width: AvailableSpace::MaxContent,
            height: space,
        },
    };
    let output = tree.compute_child_layout(
        to_node_id(key),
        LayoutInput {
            run_mode: RunMode::ComputeSize,
            sizing_mode: SizingMode::ContentSize,
            axis: match axis {
                Axis::Horizontal => RequestedAxis::Horizontal,
                Axis::Vertical => RequestedAxis::Vertical,
            },
            known_dimensions: Size::NONE,
            parent_size: Size::NONE,
            available_space: available,
            vertical_margins_are_collapsible: taffy::Line::FALSE,
        },
    );
    match axis {
        Axis::Horizontal => output.size.width,
        Axis::Vertical => output.size.height,
    }
}
