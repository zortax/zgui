//! The layout tree as the engine reads it.
//!
//! Three traits: [`LayoutTree`] answers structure, styles and leaf measurements, [`CacheAccess`]
//! hands out each box's cache, and [`Incremental`] is what the scheduler marks through. The engine
//! runs its algorithms against these and nothing else, so what a box *is* to it — a block, a flex
//! container, a leaf — is decided by the engine style the store interned for the box.

use cephal::compute::block::BlockContext;
use cephal::schedule::Incremental;
use cephal::tree::{
    AnswerSource, CacheAccess, ChildRequest, IdentSuffix, LayoutInput, LayoutOutput,
    LayoutTree as EngineTree, NodeCache, NodeId, NodeKind, QueryMeta, RunMode,
};
use cephal::{Layout, Style};
use zgui_profile::{Counter, counter};

use crate::key::{from_node_id, to_node_id};
use crate::measure::MeasureContent;
use crate::node::children::ChildIter;
use crate::node::kind::FormattingContext;
use crate::tree::LayoutTree;

/// Whether the engine may descend into a box's children.
///
/// A box that holds lines, a replaced box and a custom element are leaves to the engine: what is
/// inside them is laid out by the inline layer, the content, or the element's own source, and the
/// engine's algorithms must never see it as a list of blocks.
fn engine_sees_children(fc: FormattingContext) -> bool {
    !matches!(
        fc,
        FormattingContext::Inline
            | FormattingContext::Replaced
            | FormattingContext::Custom
            | FormattingContext::None
    )
}

impl<C: MeasureContent> EngineTree for LayoutTree<'_, C> {
    type ChildIter<'b>
        = ChildIter<'b>
    where
        Self: 'b;

    fn children(&self, node: NodeId) -> Self::ChildIter<'_> {
        let node = self.node_of(node);
        if engine_sees_children(node.fc) {
            ChildIter::new(&node.children)
        } else {
            ChildIter::new(&[])
        }
    }

    fn child_count(&self, node: NodeId) -> usize {
        let node = self.node_of(node);
        if engine_sees_children(node.fc) {
            node.children.len()
        } else {
            0
        }
    }

    fn child_at(&self, node: NodeId, index: usize) -> NodeId {
        to_node_id(self.node_of(node).children[index])
    }

    fn style(&self, node: NodeId) -> &Style {
        self.engine_style(from_node_id(node))
    }

    fn resolve_calc(&self, id: cephal::style::CalcId, basis: f32) -> f32 {
        LayoutTree::resolve_calc(self, id, basis)
    }

    fn suffixed_ident(
        &self,
        base: cephal::style::Ident,
        suffix: IdentSuffix,
    ) -> Option<cephal::style::Ident> {
        self.structure().suffixed_ident(base, suffix)
    }

    /// The engine's own derivation, read from the interned style, plus one fact the style lacks:
    /// an inline formatting context whose atomic inlines size their block axis with a percentage
    /// keeps the parent's block size in its question.
    ///
    /// Every box the engine sees no children of is answered by the engine's leaf algorithm through
    /// the [leaf shell](crate::inline::leaf_shell), so it is [`NodeKind::Leaf`]. Its measurement
    /// may read the available height, so it is not [`NodeKind::LeafIgnoringHeight`].
    #[inline]
    fn query_meta(&self, node: NodeId) -> QueryMeta {
        let key = from_node_id(node);
        let structure = self.structure();
        let (style, mut dependency) = structure.engine_style_with_dependency(key);
        let box_node = structure.node(key);
        dependency.height |= box_node.atomic_percent_block;
        let leaf = !engine_sees_children(box_node.fc) || box_node.children.is_empty();
        QueryMeta {
            hidden: style.generates_no_box(),
            // `child_count == 0 && leaf_is_cheap`, from the one record.
            cheap_leaf: matches!(
                box_node.fc,
                FormattingContext::Block | FormattingContext::Flex | FormattingContext::Grid
            ) && box_node.children.is_empty(),
            dependency,
            kind: if leaf {
                NodeKind::Leaf
            } else {
                NodeKind::of_container(style)
            },
        }
    }

    #[inline]
    fn style_plan(&self, node: NodeId) -> Option<&cephal::compute::plan::StylePlan> {
        self.structure().engine_style_plan(from_node_id(node))
    }

    #[inline]
    fn take_observed(&mut self) -> u8 {
        core::mem::take(&mut self.observed)
    }

    #[inline]
    fn note_observed(&mut self, axes: u8) {
        self.observed |= axes;
    }

    fn leaf_is_cheap(&self, node: NodeId) -> bool {
        // An empty block-level box measures nothing: its answer is its own padding and border.
        // Everything else that reaches the leaf path has content to ask, and the asking is
        // exactly what the cache exists to avoid.
        let node = self.node_of(node);
        matches!(
            node.fc,
            FormattingContext::Block | FormattingContext::Flex | FormattingContext::Grid
        ) && node.children.is_empty()
    }

    fn compute_leaf(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        crate::inline::leaf_shell::compute(self, node, inputs, block_ctx)
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout) {
        if self.speculation > 0 {
            return;
        }
        let key = from_node_id(node);
        let generation = self.generation;
        let state = self.state_mut(key);
        if state.unrounded != *layout {
            if state.cache_generation() != generation {
                let before = state.unrounded;
                state.note_touched(generation);
                self.touched.push((key, before));
            }
            let state = self.state_mut(key);
            // Until the snapping pass runs, the snapped result is the unrounded one, so a caller
            // that reads a layout between the two passes reads geometry rather than nothing.
            state.snapped = *layout;
        }
        // Written even when equal: equal layouts may differ in the sign of a zero, and the next
        // pass reads these bits back.
        self.state_mut(key).unrounded = *layout;
    }

    fn unrounded_layout(&self, node: NodeId) -> Option<Layout> {
        self.state(from_node_id(node)).map(|state| state.unrounded)
    }

    fn set_absolute_context(&mut self, node: NodeId, context: cephal::tree::AbsoluteContext) {
        if self.speculation > 0 {
            return;
        }
        self.state_mut(from_node_id(node)).absolute = Some(Box::new(context));
    }

    fn clear_absolute_context(&mut self, node: NodeId) {
        if self.speculation > 0 {
            return;
        }
        self.state_mut(from_node_id(node)).absolute = None;
    }

    fn absolute_context(&self, node: NodeId) -> Option<&cephal::tree::AbsoluteContext> {
        self.state(from_node_id(node))?.absolute.as_deref()
    }

    /// Every box claims its out-of-flow children: the box tree already parents an absolutely or
    /// fixed positioned box under its containing block (see `boxtree::absolute`), so the engine's
    /// own hoisting finds it there.
    #[inline]
    fn containing_block_claims(&self, _node: NodeId) -> u8 {
        cephal::compute::oof::OOF_ALL
    }

    /// A fixed box's parent only stands in for the viewport, which is its containing block; it
    /// scrolls with nothing, so it adds nothing to the parent's scrollable overflow.
    #[inline]
    fn out_of_flow_overflow(&self, _node: NodeId) -> u8 {
        cephal::compute::oof::OOF_ABSOLUTE
    }

    fn compute_child_layout(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        cephal::compute::compute_child_layout(self, node, inputs, block_ctx)
    }

    #[inline]
    fn forwards_child_layout(&self) -> bool {
        true
    }

    fn observe_answer(
        &mut self,
        node: NodeId,
        inputs: &LayoutInput,
        output: &LayoutOutput,
        source: AnswerSource,
    ) {
        observe(self, node, inputs, output, source);
    }

    fn batches_help(&self) -> bool {
        self.batch_pool().is_some()
    }

    fn compute_child_layouts(&mut self, requests: &[ChildRequest], out: &mut Vec<LayoutOutput>) {
        self.run_batch(requests, out);
    }
}

impl<C> CacheAccess for LayoutTree<'_, C> {
    fn cache(&self, node: NodeId) -> &NodeCache {
        &self
            .state(from_node_id(node))
            .expect("every box the engine asks about holds layout state")
            .cache
    }

    fn cache_mut(&mut self, node: NodeId) -> &mut NodeCache {
        &mut self.state_mut(from_node_id(node)).cache
    }

    fn generation(&self) -> u32 {
        self.generation
    }

    fn speculation(&self) -> u32 {
        self.speculation
    }

    fn set_speculation(&mut self, depth: u32) {
        self.speculation = depth;
    }
}

impl<C: MeasureContent> Incremental for LayoutTree<'_, C> {
    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.structure()
            .get(from_node_id(node))?
            .parent
            .map(to_node_id)
    }

    fn depth(&self, node: NodeId) -> u32 {
        // A box recycled since it was queued has no record and sits at the root's depth.
        let structure = self.structure();
        let mut depth = 0;
        let mut at = structure
            .get(from_node_id(node))
            .and_then(|node| node.parent);
        while let Some(parent) = at {
            depth += 1;
            at = structure.get(parent).and_then(|node| node.parent);
        }
        depth
    }

    fn flags(&self, node: NodeId) -> u8 {
        self.state(from_node_id(node))
            .map_or(0, |state| state.flags)
    }

    fn set_flags(&mut self, node: NodeId, flags: u8) {
        // A box removed since it was queued has no state; the scheduler skips it next.
        let key = from_node_id(node);
        if self.state(key).is_some() {
            self.state_mut(key).flags = flags;
        }
    }
}

/// Accounts for one answer and records the baselines it carries, on every answer the engine
/// hands out.
///
/// The baselines are recorded on every answer, because an answer may come from the cache without
/// any algorithm running. A box whose baselines were recorded by one pass and served from the
/// cache in the next would otherwise keep whatever the first pass happened to see.
fn observe<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    node: NodeId,
    inputs: &LayoutInput,
    output: &LayoutOutput,
    source: AnswerSource,
) {
    let cached = source == AnswerSource::Cached;
    match (inputs.run_mode, cached) {
        (RunMode::PerformLayout, false) => counter::bump(Counter::NodesRelaidOut),
        (RunMode::ComputeSize, false) => counter::bump(Counter::SizesMeasured),
        (RunMode::ComputeSize, true) => counter::bump(Counter::SizesHeld),
        _ => {}
    }
    let first = output.baselines.first;
    let state = tree.state_mut(from_node_id(node));
    state.first_baseline = first;
    state.last_baseline = output.baselines.last.or(first);
}
