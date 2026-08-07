//! Dispatching one box to the algorithm that lays it out, and filling in what that algorithm
//! leaves out.
//!
//! # The baseline fill-in, and why it is never an overwrite
//!
//! Baseline alignment reads a child's first baseline off the result the child returned. A leaf
//! returns none, and so does a block container — but a *flex* container computes a real one, from
//! the first baseline-aligned item on its first line. So the fill-in is `or_else` and never an
//! assignment: an assignment would destroy a flex container's own answer and replace it with one
//! derived from its first child, which is a different box whenever any item is baseline-aligned.
//!
//! A last baseline is carried beside the first, because CSS aligns an `inline-block` in normal flow
//! on its *last* line box and the algorithms only ever report a first. Without it a multi-line
//! `inline-block` in a baseline-aligned row sits one line too high.

use taffy::{
    CoreStyle, LayoutBlockContainer, LayoutFlexboxContainer, LayoutGridContainer, LayoutInput,
    LayoutOutput, LayoutPartialTree, NodeId, RunMode,
};
use zgui_dom::side::BoxKey;
use zgui_profile::{Counter, counter};

use crate::key::from_node_id;
use crate::measure::MeasureContent;
use crate::node::kind::FormattingContext;
use crate::style::StyleRef;
use crate::tree::LayoutTree;

impl<C: MeasureContent> LayoutPartialTree for LayoutTree<'_, C> {
    type CoreContainerStyle<'a>
        = StyleRef<'a>
    where
        Self: 'a;
    type CustomIdent = zgui_interned::Ident;

    fn get_core_container_style(&self, node: NodeId) -> StyleRef<'_> {
        self.style_of(from_node_id(node))
    }

    fn resolve_calc_value(&self, value: *const (), basis: f32) -> f32 {
        crate::style::calc::resolve_in(self.calc_arena(), value, basis)
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &taffy::Layout) {
        let key = from_node_id(node);
        let state = self.store_mut().state_mut(key);
        state.unrounded = *layout;
        // Until the snapping pass runs, the snapped result is the unrounded one, so a caller that
        // reads a layout between the two passes reads geometry rather than nothing.
        state.snapped = *layout;
    }

    fn compute_child_layout(&mut self, node: NodeId, inputs: LayoutInput) -> LayoutOutput {
        dispatch(self, node, inputs, None)
    }
}

impl<C: MeasureContent> LayoutFlexboxContainer for LayoutTree<'_, C> {
    type FlexboxContainerStyle<'a>
        = StyleRef<'a>
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = StyleRef<'a>
    where
        Self: 'a;

    fn get_flexbox_container_style(&self, node: NodeId) -> StyleRef<'_> {
        self.style_of(from_node_id(node))
    }

    fn get_flexbox_child_style(&self, child: NodeId) -> StyleRef<'_> {
        self.style_of(from_node_id(child))
    }
}

impl<C: MeasureContent> LayoutGridContainer for LayoutTree<'_, C> {
    type GridContainerStyle<'a>
        = StyleRef<'a>
    where
        Self: 'a;
    type GridItemStyle<'a>
        = StyleRef<'a>
    where
        Self: 'a;

    fn get_grid_container_style(&self, node: NodeId) -> StyleRef<'_> {
        self.style_of(from_node_id(node))
    }

    fn get_grid_child_style(&self, child: NodeId) -> StyleRef<'_> {
        self.style_of(from_node_id(child))
    }
}

impl<C: MeasureContent> LayoutBlockContainer for LayoutTree<'_, C> {
    type BlockContainerStyle<'a>
        = StyleRef<'a>
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = StyleRef<'a>
    where
        Self: 'a;

    fn get_block_container_style(&self, node: NodeId) -> StyleRef<'_> {
        self.style_of(from_node_id(node))
    }

    fn get_block_child_style(&self, child: NodeId) -> StyleRef<'_> {
        self.style_of(from_node_id(child))
    }

    fn compute_block_child_layout(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> LayoutOutput {
        // Overridden rather than left to its default, which drops the block context: floats and
        // margin collapsing degrade silently across nested blocks without it, with no error and no
        // sign in any result that anything was lost.
        dispatch(self, node, inputs, block_ctx)
    }
}

/// Lays one box out, from the cache if the same question has already been answered.
fn dispatch<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    node: NodeId,
    inputs: LayoutInput,
    block_ctx: Option<&mut taffy::BlockContext<'_>>,
) -> LayoutOutput {
    if inputs.run_mode == RunMode::PerformHiddenLayout {
        return taffy::compute_hidden_layout(tree, node);
    }
    let output = taffy::compute_cached_layout(tree, node, inputs, |tree, node, inputs| {
        let key = from_node_id(node);
        counter::bump(Counter::NodesRelaidOut);
        let fc = tree.store().node(key).fc;
        let generates_no_box =
            tree.style_of(key).box_generation_mode() == taffy::BoxGenerationMode::None;
        let (mut output, last_baseline) = if generates_no_box {
            (taffy::compute_hidden_layout(tree, node), None)
        } else {
            match fc {
                FormattingContext::None => (taffy::compute_hidden_layout(tree, node), None),
                FormattingContext::Flex => {
                    (taffy::compute_flexbox_layout(tree, node, inputs), None)
                }
                FormattingContext::Grid => (taffy::compute_grid_layout(tree, node, inputs), None),
                // Tables and multi-column boxes are laid out as block containers until their own
                // algorithms exist, which keeps their children on the page rather than at zero.
                FormattingContext::Block
                | FormattingContext::Table
                | FormattingContext::MultiColumn => (
                    taffy::compute_block_layout(tree, node, inputs, block_ctx),
                    None,
                ),
                // An atomic inline is a leaf to the line it sits in and a container to what is
                // inside it. Reached here it is being laid out as a container, so it runs the
                // algorithm its inner display names; the leaf half is what the line asks for.
                FormattingContext::Atomic => {
                    let inner = crate::style::convert::display::atomic_inner(
                        tree.store().node(key).style.get_box().display,
                    );
                    match inner {
                        FormattingContext::Flex => {
                            (taffy::compute_flexbox_layout(tree, node, inputs), None)
                        }
                        FormattingContext::Grid => {
                            (taffy::compute_grid_layout(tree, node, inputs), None)
                        }
                        _ => (taffy::compute_block_layout(tree, node, inputs, None), None),
                    }
                }
                FormattingContext::Inline
                | FormattingContext::Replaced
                | FormattingContext::Custom => {
                    // The block context travels with it: an inline formatting context asks the
                    // floats around it how wide each of its lines may be, and a leaf that was
                    // handed no context would lay its lines out straight through them. A custom
                    // element takes the same road because it *is* a leaf to CSS: the shell
                    // arithmetic around the measurement — box-sizing, min and max, aspect ratio —
                    // applies to it unchanged, and only the measurement itself is the trait's.
                    crate::inline::leaf_shell::compute(tree, node, inputs, block_ctx)
                }
            }
        };
        fill_in_baselines(tree, key, fc, &mut output, last_baseline);
        output
    });
    // Again on the way out, because the answer above may have been served from the cache without
    // the closure running at all. A box whose baselines were recorded by one pass and served from
    // the cache in the next would otherwise keep whatever the first pass happened to see, which is
    // how an incremental layout comes to disagree with a fresh one about where a row sits.
    if inputs.run_mode == RunMode::PerformLayout {
        let key = from_node_id(node);
        let fc = tree.store().node(key).fc;
        record_baselines(tree, key, fc, &output);
    }
    output
}

/// Records the baselines one box reported, and the one it inherits from its content.
fn record_baselines<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    fc: FormattingContext,
    output: &LayoutOutput,
) {
    let first = output
        .first_baselines
        .y
        .or_else(|| first_baseline_of_content(tree, key, fc));
    let last = last_baseline_of_content(tree, key, fc).or(first);
    let state = tree.store_mut().state_mut(key);
    state.first_baseline = first;
    state.last_baseline = last;
}

/// Supplies the baselines the algorithms do not report, and records both on the box.
fn fill_in_baselines<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    fc: FormattingContext,
    output: &mut LayoutOutput,
    measured_last: Option<f32>,
) {
    if output.first_baselines.y.is_none() {
        output.first_baselines.y = first_baseline_of_content(tree, key, fc);
    }
    let last = measured_last.or_else(|| last_baseline_of_content(tree, key, fc));
    let state = tree.store_mut().state_mut(key);
    state.first_baseline = output.first_baselines.y;
    state.last_baseline = last.or(output.first_baselines.y);
}

/// The rules a box actually laid its children out by.
///
/// An atomic inline is tagged by how the context *around* it sees it, and that tag says nothing
/// about what ran inside it: an `inline-block` runs block layout, an `inline-flex` runs flexbox.
/// Every question about what a box's own content produced — its baselines above all — is a question
/// about the inner context, so it is resolved here rather than asked of the outer tag. Asking the
/// outer tag leaves an `inline-block` with no baseline at all, and a box with no baseline sits on a
/// line by its bottom margin edge instead of by its text.
fn inner_context<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    key: BoxKey,
    fc: FormattingContext,
) -> FormattingContext {
    if fc != FormattingContext::Atomic {
        return fc;
    }
    crate::style::convert::display::atomic_inner(tree.store().node(key).style.get_box().display)
}

/// The baseline a container inherits from its first in-flow child.
///
/// Only block containers ask: a flex container has already computed its own, and asking here would
/// give a different and wrong answer whenever any of its items is baseline-aligned.
fn first_baseline_of_content<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    key: BoxKey,
    fc: FormattingContext,
) -> Option<f32> {
    if inner_context(tree, key, fc) != FormattingContext::Block {
        return None;
    }
    let children = tree.store().node(key).children.clone();
    children.iter().find_map(|&child| {
        if !is_in_flow(tree, child) {
            return None;
        }
        let state = tree.store().state(child)?;
        let baseline = state.first_baseline?;
        Some(baseline + state.unrounded.location.y)
    })
}

/// Whether a child takes part in its parent's own flow.
///
/// A float and an absolutely positioned box are laid out beside the flow rather than in it, so
/// neither is a box the container takes its baseline from — a floated pull-quote in a large face
/// before the first paragraph would otherwise decide where a whole baseline-aligned row sits.
fn is_in_flow<C: MeasureContent>(tree: &LayoutTree<'_, C>, child: BoxKey) -> bool {
    let style = tree.style_of(child);
    taffy::BlockItemStyle::float(&style) == taffy::Float::None
        && CoreStyle::position(&style) != taffy::Position::Absolute
}

/// The last baseline a box takes: its own last line box, or its last in-flow child's.
fn last_baseline_of_content<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    key: BoxKey,
    fc: FormattingContext,
) -> Option<f32> {
    if fc == FormattingContext::Inline {
        // A box that holds lines has a last baseline of its own, and it is not the same line as
        // its first whenever the content wrapped.
        let state = tree.store().state(key)?;
        let inset = state.unrounded.border.top + state.unrounded.padding.top;
        return tree
            .store()
            .inline_resolution(key)
            .and_then(crate::inline::resolved::InlineResolution::last_baseline)
            .map(|baseline| baseline + inset);
    }
    if !inner_context(tree, key, fc).is_container() {
        return None;
    }
    let children = tree.store().node(key).children.clone();
    children.iter().rev().find_map(|&child| {
        if !is_in_flow(tree, child) {
            return None;
        }
        let state = tree.store().state(child)?;
        let baseline = state.last_baseline?;
        Some(baseline + state.unrounded.location.y)
    })
}
