//! The per-box layout cache.
//!
//! One box holds one full-layout slot and nine size-only slots, so the repeated probing the flex
//! and grid algorithms do — a minimum-content pass, a maximum-content pass and then a definite one,
//! per item — costs one computation each rather than one per probe.
//!
//! # The second storey, and the questions the nine slots cannot hold
//!
//! The nine slots are chosen by the *shape* of the question rather than by the question, so two
//! min-content probes taken against two different grid areas share one slot and the second evicts
//! the first. Grid track sizing asks exactly that pair, several times over, per item, per axis. So a
//! miss on the nine slots falls through to a second storey, which is the same cache keyed on the
//! whole question and with as many slots as the questions asked; and a store fills both. The two
//! are emptied by the same call, and nothing may empty one without the other.
//!
//! # Why a served answer is completed rather than returned as it is
//!
//! A size-only slot holds a size and nothing else, so an answer served from one carries no
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
        let state = self.store().state(key)?;
        let mut output = match state.cache.get(input) {
            Some(output) => output,
            // Only a size-only question falls through. A full layout is held in one slot on purpose:
            // performing one writes geometry onto everything below the box, and serving an older
            // one from a wider store would return the right size while leaving the descendants
            // placed for the question asked after it.
            None if input.run_mode == RunMode::ComputeSize => {
                let size = state.measured.get(input)?;
                counter::bump(Counter::SizesHeld);
                LayoutOutput::from_outer_size(size)
            }
            None => return None,
        };
        if input.run_mode == RunMode::ComputeSize && output.first_baselines.y.is_none() {
            output.first_baselines.y = state.first_baseline;
        }
        Some(output)
    }

    fn cache_store(&mut self, node: NodeId, input: &LayoutInput, output: LayoutOutput) {
        let state = self.store_mut().state_mut(from_node_id(node));
        state.cache.store(input, output);
        if input.run_mode == RunMode::ComputeSize {
            counter::bump(Counter::SizesMeasured);
            state.measured.insert(input, output.size);
        }
    }

    fn cache_clear(&mut self, node: NodeId) {
        self.store_mut()
            .state_mut(from_node_id(node))
            .forget_layout();
    }
}
