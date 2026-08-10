//! Finishing a frame: planning the vector passes and sorting the remap lists into draw order.

use zgui_bits::DamageSet;
use zgui_profile::{Counter, counter};

use crate::pass::coalesce::{self, Event, Input};
use crate::pass::overlap::Overlap;
use crate::prim::PrimitiveKind;
use crate::scene::Scene;

impl Scene {
    /// Finishes the frame: plans the vector passes against `damage` and sorts the remap lists
    /// into draw order.
    ///
    /// Uses the policy's own reading of the overlap rule. [`Scene::finish_with`] is the version
    /// that takes another, which exists so the readings can be compared rather than asserted.
    pub fn finish(&mut self, damage: &DamageSet) {
        self.finish_with(damage, Overlap::default());
    }

    /// Finishes the frame using a chosen reading of the vector-pass overlap rule.
    pub fn finish_with(&mut self, damage: &DamageSet, overlap: Overlap) {
        // Before the sort, because the waiting list is indexed into the arrays and the sort key's
        // first component after draw order is the texture, which a placeholder does not have.
        self.refuse_unresolved();
        // Before the sort as well: the sweep is over the emission stream, and the log's indices
        // are the arrays' own for the whole of the frame — nothing rewrites them.
        self.plan_vector_passes(damage, overlap);
        self.sort_remap();
        self.finished = true;
        self.check_order_overlap();
    }

    /// Sorts every remap list into draw order, leaving the arrays as they were pushed.
    ///
    /// A primitive's position in its array is its identity for the frame — the log names it, and
    /// a persistent copy of it elsewhere names it — so the ordering is a list of indices beside
    /// the array rather than a move of the structs. A batch is a range of the sorted list, and a
    /// consumer reads the array through it.
    fn sort_remap(&mut self) {
        sort_lane(&mut self.remap.quads, &self.primitives.quads, |quad| {
            (quad.order, 0, 0)
        });
        sort_lane(
            &mut self.remap.shadows,
            &self.primitives.shadows,
            |shadow| (shadow.order, 0, 0),
        );
        sort_lane(
            &mut self.remap.decorations,
            &self.primitives.decorations,
            |decoration| (decoration.order, 0, 0),
        );
        // Sprites break their tie by texture first and tile second. Two sprites at equal draw
        // order are provably non-overlapping, so their sequence is free; spending it on
        // clustering by texture is what lets a batch run until the texture genuinely changes.
        sort_lane(
            &mut self.remap.mono_sprites,
            &self.primitives.mono_sprites,
            |sprite| {
                let (texture, tile) = sprite.tile.sort_key();
                (sprite.order, texture, tile)
            },
        );
        sort_lane(
            &mut self.remap.subpixel_sprites,
            &self.primitives.subpixel_sprites,
            |sprite| {
                let (texture, tile) = sprite.tile.sort_key();
                (sprite.order, texture, tile)
            },
        );
        sort_lane(
            &mut self.remap.color_sprites,
            &self.primitives.color_sprites,
            |sprite| {
                let (texture, tile) = sprite.tile.sort_key();
                (sprite.order, texture, tile)
            },
        );
        sort_lane(
            &mut self.remap.externals,
            &self.primitives.externals,
            |external| (external.order, 0, 0),
        );
        sort_lane(
            &mut self.remap.backdrops,
            &self.primitives.backdrops,
            |backdrop| (backdrop.order, 0, 0),
        );
        // A degenerate empty group has its start and end at the same order; the start has to come
        // first, or the pair is inverted and the target is composited before it exists.
        sort_lane(&mut self.remap.groups, &self.primitives.groups, |group| {
            (group.order, u32::from(!group.is_start), 0)
        });
        // Vector items keep their emission order: the pass policy sweeps them in it, and their
        // composites are ordered by the pass plan rather than by any array.

        self.index_markers();
    }

    /// Records where each direction's markers sit in the array they share, in draw order.
    ///
    /// Built from the sorted remap, and read by the batcher, which walks starts and ends as two
    /// streams and has to turn a cursor into either one back into a position in the shared array.
    fn index_markers(&mut self) {
        self.markers.clear();
        for &position in &self.remap.groups {
            if self.primitives.groups[position as usize].is_start {
                self.markers.starts.push(position);
            } else {
                self.markers.ends.push(position);
            }
        }
    }

    /// Runs the coalescing policy and records what it cost.
    fn plan_vector_passes(&mut self, damage: &DamageSet, overlap: Overlap) {
        // No vector items, no passes — and then the sweep below is a walk of the whole emission
        // stream, one `ink_of` per entry and a `Vec` as long as it, to arrive at an empty plan.
        // Every event it could raise is either a vector or something a vector is coalesced
        // against, so with nothing to coalesce the answer is the cleared plan.
        if self.primitives.vectors.is_empty() {
            self.pass_plan.clear();
            return;
        }

        // The sweep is over the emission stream, which is what rule 3 is stated in terms of. It
        // agrees with draw order wherever the question can matter: anything overlapping something
        // already pushed was given a strictly higher order than it, so for every overlapping pair
        // the two orders are the same order.
        let events: Vec<Event> = self
            .ops
            .iter()
            .map(|op| match op.kind {
                PrimitiveKind::Vector => Event::Vector(op.index as usize),
                // A group marker is where the renderer changes target, which a pass may not span.
                // It is emphatically not an occluder: what a group covers is irrelevant to the
                // question, and treating it as one let a pass run straight through the boundary.
                PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => Event::Boundary,
                _ => Event::Occluder(self.ink_of(*op)),
            })
            .collect();

        coalesce::plan(
            Input {
                events: &events,
                vectors: &self.primitives.vectors,
                damage,
                viewport: self.viewport,
                overlap,
            },
            &mut self.clips,
            &mut self.pass_plan,
        );

        // Composites are drawn in draw order like everything else, so the passes are kept in it.
        self.pass_plan
            .passes
            .sort_by_key(|pass| pass.composite_order);

        counter::add(Counter::VelloPasses, self.pass_plan.passes.len() as u64);
        counter::add(Counter::VectorClipLayers, self.pass_plan.clip_layers as u64);
    }
}

/// Fills `lane` with every index of `values`, sorted by `key`.
fn sort_lane<T, K: Fn(&T) -> (u32, u32, u32)>(lane: &mut Vec<u32>, values: &[T], key: K) {
    lane.clear();
    lane.extend(0..values.len() as u32);
    lane.sort_by_key(|index| key(&values[*index as usize]));
}
