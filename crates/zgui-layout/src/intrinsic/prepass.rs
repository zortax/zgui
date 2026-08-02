//! Measuring the boxes whose sizes are written as content keywords, before laying anything out.
//!
//! The pass visits only the boxes that need it. In a document that writes no content keyword — the
//! overwhelming majority — it walks the tree once and measures nothing, and the memo it leaves is
//! empty.
//!
//! # Deepest first
//!
//! A box written `max-content` inside a box written `max-content` has to be measured *before* the
//! box that contains it: until its own keyword has an answer it reads as `auto`, and a container
//! measured while its child says `auto` is measured against a child that will not be that size.
//! So the boxes that need measuring are visited in reverse tree order, and every answer is in place
//! before anything above it is asked.
//!
//! Each answer also invalidates the cache of the box it belongs to. The probes that produced it ran
//! while the keyword still read as `auto`, and an entry cached under that reading would otherwise
//! be handed back during the real layout, when the keyword means a length.

use taffy::{
    AvailableSpace, CacheTree, LayoutInput, LayoutPartialTree, RequestedAxis, RunMode, Size,
    SizingMode,
};
use zgui_dom::side::BoxKey;

use crate::axis::Axis;
use crate::intrinsic::keywords::axes_needing_measurement;
use crate::key::to_node_id;
use crate::measure::MeasureContent;
use crate::style::convert::length::IntrinsicSizes;
use crate::tree::LayoutTree;

/// Measures every box under `root` that is sized by a content keyword.
pub fn run<C: MeasureContent>(tree: &mut LayoutTree<'_, C>, root: BoxKey) {
    let mut needed: Vec<(BoxKey, [bool; 2])> = Vec::new();
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        let axes = axes_needing_measurement(tree.style_of(key));
        if axes[0] || axes[1] {
            needed.push((key, axes));
        }
        stack.extend(tree.store().node(key).children.iter().copied());
    }
    // The walk above is a pre-order one, so every box precedes its descendants in it. Reversing it
    // therefore answers every descendant before the box that contains it.
    for (key, axes) in needed.into_iter().rev() {
        for (index, axis) in Axis::BOTH.into_iter().enumerate() {
            if axes[index] {
                let sizes = measure(tree, key, axis);
                tree.intrinsic_mut().insert(key, axis, sizes);
            }
        }
        tree.cache_clear(to_node_id(key));
    }
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
