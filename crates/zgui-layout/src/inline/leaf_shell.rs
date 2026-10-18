//! The engine's leaf algorithm around a measurement that needs the tree.
//!
//! A box that holds lines, an atomic inline and a custom element all need the tree while they
//! measure — an atomic's content is a nested layout of boxes this tree owns — so they measure
//! through a closure over the tree rather than the engine's shared `measure` hook. The box
//! arithmetic around the measurement — percentage resolution, `box-sizing`, the min/max clamp, the
//! scrollbar gutter, the available-space derivation and the aspect-ratio fix-up — is the engine's
//! own, so it reads the engine's resolved styles and records what the answer observed exactly as
//! the engine's leaves do.

use cephal::compute::block::BlockContext;
use cephal::compute::leaf::{LeafBox, compute_empty_leaf_layout, compute_leaf_layout_with};
use cephal::tree::{
    Baselines, LayoutInput, LayoutOutput, LayoutTree as EngineTree, MeasureOutput, NodeId,
    SizingMode,
};
use cephal::{MaybeMath, Size};

use crate::key::from_node_id;
use crate::measure::MeasureContent;
use crate::node::kind::{BoxKind, FormattingContext};
use crate::tree::LayoutTree;

/// Lays out one leaf, measuring its content through the tree.
pub(crate) fn compute<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    node: NodeId,
    inputs: LayoutInput,
    block: Option<&mut BlockContext<'_>>,
) -> LayoutOutput {
    // An empty block-level box measures nothing: its answer is its own box.
    if matches!(
        tree.node_of(node).fc,
        FormattingContext::Block | FormattingContext::Flex | FormattingContext::Grid
    ) {
        return compute_empty_leaf_layout(tree, node, inputs);
    }
    let basis_height = percentage_basis_height(tree, node, inputs);
    let LayoutInput { run_mode, axis, .. } = inputs;
    compute_leaf_layout_with(tree, node, inputs, true, |tree, input| {
        // The algorithm recorded the axes the measurement may tell min-content from max-content
        // on, and the content reads the question through `input` alone. What the nested layouts
        // below observe is about questions the content chose, so it is dropped, which also keeps
        // the record independent of which nested answers were memoised.
        let outer = EngineTree::take_observed(tree);
        let measured = crate::inline::measure_leaf(
            tree,
            node,
            input.known_dimensions,
            input.available_space,
            run_mode,
            axis,
            basis_height,
            block,
        );
        EngineTree::take_observed(tree);
        EngineTree::note_observed(tree, outer);
        MeasureOutput {
            size: measured.size,
            baselines: Baselines {
                first: measured.first_baseline,
                last: measured.last_baseline,
            },
        }
    })
}

/// The block size a percentage height of an atomic inline in this context resolves against.
///
/// That is the height of the containing block. An anonymous inline root stands in for the block
/// container around it, so the parent's size is the basis. Another context root is its own block
/// container, so its definite content height is the basis. A context without such an atomic gets
/// nothing, which keeps its answers independent of any height.
fn percentage_basis_height<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    node: NodeId,
    inputs: LayoutInput,
) -> Option<f32> {
    let key = from_node_id(node);
    let box_node = tree.structure().node(key);
    if !box_node.atomic_percent_block {
        return None;
    }
    if box_node.kind == BoxKind::AnonymousInlineRoot {
        return inputs.parent_size.height;
    }
    let leaf = match tree.structure().engine_style_plan(key) {
        Some(plan) => plan.leaf,
        None => {
            let calc = |id, basis| tree.resolve_calc(id, basis);
            LeafBox::resolve(tree.engine_style(key), inputs.parent_size, &calc)
        }
    };
    let known = inputs.known_dimensions;
    let (size, min_size, max_size) = match inputs.sizing_mode {
        SizingMode::ContentSize => (known, Size::NONE, Size::NONE),
        SizingMode::InherentSize => (known.or(leaf.size), leaf.min_size, leaf.max_size),
    };
    size.height.map(|height| {
        (height.maybe_clamp(min_size.height, max_size.height)
            - leaf.content_box_inset.vertical_axis_sum())
        .max(0.0)
    })
}
