//! Finishing a frame: sorting the arrays, remapping the log, and planning the vector passes.

use zgui_bits::DamageSet;
use zgui_profile::{Counter, counter};

use crate::pass::coalesce::{self, Event, Input};
use crate::pass::overlap::Overlap;
use crate::prim::PrimitiveKind;
use crate::scene::Scene;

impl Scene {
    /// Finishes the frame: sorts the arrays into draw order and plans the vector passes against
    /// `damage`.
    ///
    /// Uses the policy's own reading of the overlap rule. [`Scene::finish_with`] is the version
    /// that takes another, which exists so the readings can be compared rather than asserted.
    pub fn finish(&mut self, damage: &DamageSet) {
        self.finish_with(damage, Overlap::default());
    }

    /// Finishes the frame using a chosen reading of the vector-pass overlap rule.
    pub fn finish_with(&mut self, damage: &DamageSet, overlap: Overlap) {
        // Before the sort, because the waiting list is indexed into the arrays as they were pushed
        // and the sort is what moves them — and because the sort key's first component after draw
        // order is the texture, which a placeholder does not have.
        self.refuse_unresolved();
        self.sort_and_remap();
        self.plan_vector_passes(damage, overlap);
        self.finished = true;
        // After the sort, because it is the sorted arrays a batch is taken out of and the sort is
        // what the invariant licenses. Costs nothing at all unless the checks were asked for.
        self.check_order_overlap();
    }

    /// Sorts every array by draw order and rewrites the log to match.
    ///
    /// The log records where each primitive was *inserted*, and sorting moves it. Rewriting the log
    /// rather than leaving it is what keeps a recorded range meaningful after the frame is
    /// finished — which is the only state anything outside this crate sees.
    fn sort_and_remap(&mut self) {
        let quads = sort_by(&mut self.primitives.quads, |quad| (quad.order, 0, 0));
        let shadows = sort_by(&mut self.primitives.shadows, |shadow| (shadow.order, 0, 0));
        let decorations = sort_by(&mut self.primitives.decorations, |decoration| {
            (decoration.order, 0, 0)
        });
        // Sprites break their tie by texture first and tile second. Two sprites at equal draw order
        // are provably non-overlapping, so their sequence is free; spending it on clustering by
        // texture is what lets a batch run until the texture genuinely changes.
        let mono = sort_by(&mut self.primitives.mono_sprites, |sprite| {
            let (texture, tile) = sprite.tile.sort_key();
            (sprite.order, texture, tile)
        });
        let subpixel = sort_by(&mut self.primitives.subpixel_sprites, |sprite| {
            let (texture, tile) = sprite.tile.sort_key();
            (sprite.order, texture, tile)
        });
        let color = sort_by(&mut self.primitives.color_sprites, |sprite| {
            let (texture, tile) = sprite.tile.sort_key();
            (sprite.order, texture, tile)
        });
        let externals = sort_by(&mut self.primitives.externals, |external| {
            (external.order, 0, 0)
        });
        let backdrops = sort_by(&mut self.primitives.backdrops, |backdrop| {
            (backdrop.order, 0, 0)
        });
        // A degenerate empty group has its start and end at the same order; the start has to come
        // first, or the pair is inverted and the target is composited before it exists.
        let groups = sort_by(&mut self.primitives.groups, |group| {
            (group.order, u32::from(!group.is_start), 0)
        });
        // Vector items keep their emission order: the pass policy sweeps them in it, and their
        // composites are ordered by the pass plan rather than by this array.

        for op in &mut self.ops {
            let mapping = match op.kind {
                PrimitiveKind::Quad => &quads,
                PrimitiveKind::Shadow => &shadows,
                PrimitiveKind::Decoration => &decorations,
                PrimitiveKind::MonoSprite => &mono,
                PrimitiveKind::SubpixelSprite => &subpixel,
                PrimitiveKind::ColorSprite => &color,
                PrimitiveKind::External => &externals,
                PrimitiveKind::Backdrop => &backdrops,
                PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => &groups,
                PrimitiveKind::Vector => continue,
            };
            op.index = mapping[op.index as usize];
        }
    }

    /// Runs the coalescing policy and records what it cost.
    fn plan_vector_passes(&mut self, damage: &DamageSet, overlap: Overlap) {
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

/// Sorts `values` by `key` and returns, for each old index, where the value ended up.
fn sort_by<T, K: Fn(&T) -> (u32, u32, u32)>(values: &mut Vec<T>, key: K) -> Vec<u32> {
    let mut order: Vec<u32> = (0..values.len() as u32).collect();
    order.sort_by_key(|index| key(&values[*index as usize]));

    let mut mapping = vec![0u32; values.len()];
    for (position, old) in order.iter().enumerate() {
        mapping[*old as usize] = position as u32;
    }

    let mut sorted: Vec<Option<T>> = values.drain(..).map(Some).collect();
    for old in order {
        values.push(
            sorted[old as usize]
                .take()
                .expect("each index appears once in a permutation"),
        );
    }
    mapping
}
