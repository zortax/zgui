//! The per-box layout cache.
//!
//! One box holds one full-layout answer and sixteen complete size-only answers. Taffy's former nine
//! approximate size slots duplicated those answers while omitting parts of the question; keeping
//! the complete cache alone saves the per-box array without repeating flex and grid probes.
//!
//! # Why a served answer is completed rather than returned as it is
//!
//! A size-only answer holds a size and nothing else, so an answer served from one carries no
//! baseline — while the same question answered by computing it does. That difference is invisible
//! in a document laid out once and decides where a baseline-aligned row sits in one laid out twice:
//! the second pass hits the slot the first pass filled, the container above reads no baseline from
//! its item, and a row that was aligned on its text aligns on its bottom edge instead. So the
//! baseline the box last reported is put back, which is the same number the computation would have
//! produced and is why a re-layout agrees with a fresh one.

use taffy::{CacheTree, LayoutInput, LayoutOutput, NodeId, RunMode};
use zgui_profile::{Counter, counter};

use crate::key::from_node_id;
use crate::tree::LayoutTree;

impl<C> CacheTree for LayoutTree<'_, C> {
    fn cache_get(&self, node: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        let key = from_node_id(node);
        let state = self.state(key)?;
        let mut output = match input.run_mode {
            RunMode::PerformLayout => state.full.get(input)?,
            RunMode::ComputeSize => {
                let size = state.measured.get(input)?;
                counter::bump(Counter::SizesHeld);
                LayoutOutput::from_outer_size(size)
            }
            RunMode::PerformHiddenLayout => return None,
        };
        if input.run_mode == RunMode::ComputeSize && output.first_baselines.y.is_none() {
            output.first_baselines.y = state.first_baseline;
        }
        Some(output)
    }

    fn cache_store(&mut self, node: NodeId, input: &LayoutInput, output: LayoutOutput) {
        let state = self.state_mut(from_node_id(node));
        match input.run_mode {
            RunMode::PerformLayout => state.full.store(input, output),
            RunMode::ComputeSize => {
                counter::bump(Counter::SizesMeasured);
                state.measured.insert(input, output.size);
            }
            RunMode::PerformHiddenLayout => {}
        }
    }

    fn cache_clear(&mut self, node: NodeId) {
        self.store_mut()
            .state_mut(from_node_id(node))
            .forget_layout();
    }
}
