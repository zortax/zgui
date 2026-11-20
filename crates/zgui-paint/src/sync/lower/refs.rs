//! The nodes and slots one chunk's primitives refer to.

use compositor_types::{
    ChunkRecorder, ClipId, ClipRef, Color, PaintId, PaintKind, PaintRef, SpaceRef, Transaction,
    TransformId,
};
use rustc_hash::FxHashMap;
use zgui_scene::{ChunkPrims, Scene};

use crate::sync::mirror::Mirror;

/// The compositor nodes and slots of one chunk, by zgui slot and id.
#[derive(Debug, Default)]
pub(crate) struct Refs {
    spaces: FxHashMap<u32, TransformId>,
    clips: FxHashMap<u32, Option<ClipId>>,
    paints: FxHashMap<u32, Option<(PaintId, PaintKind)>>,
    solids: FxHashMap<[u32; 4], PaintId>,
}

/// The references one chunk's items hold, minted once per id by the chunk's recorder.
#[derive(Debug, Default)]
pub(crate) struct Recorded {
    spaces: FxHashMap<u32, SpaceRef>,
    clips: FxHashMap<u32, ClipRef>,
    paints: FxHashMap<u32, PaintRef>,
    solids: FxHashMap<[u32; 4], PaintRef>,
}

impl Refs {
    /// Mirrors everything `chunk` refers to.
    pub(crate) fn gather(
        &mut self,
        tx: &mut Transaction,
        mirror: &mut Mirror,
        scene: &mut Scene,
        chunk: &ChunkPrims,
    ) {
        for quad in &chunk.quads {
            self.space(tx, mirror, scene, quad.transform);
            self.clip(tx, mirror, scene, quad.clip);
            self.paint(tx, mirror, scene, quad.fill);
            self.paint(tx, mirror, scene, quad.stroke);
        }
        for shadow in &chunk.shadows {
            self.space(tx, mirror, scene, shadow.transform);
            self.clip(tx, mirror, scene, shadow.clip);
        }
        for line in &chunk.decorations {
            self.space(tx, mirror, scene, line.transform);
            self.clip(tx, mirror, scene, line.clip);
        }
        for run in &chunk.glyph_runs {
            self.space(tx, mirror, scene, run.transform);
            self.clip(tx, mirror, scene, run.clip);
            let color = Color(run.color);
            let bits = color.0.map(f32::to_bits);
            self.solids
                .entry(bits)
                .or_insert_with(|| mirror.paints.solid(tx, color));
        }
    }

    /// Mirrors the coordinate system of `slot`, and returns its node.
    pub(crate) fn space(
        &mut self,
        tx: &mut Transaction,
        mirror: &mut Mirror,
        scene: &Scene,
        slot: u32,
    ) -> TransformId {
        *self
            .spaces
            .entry(slot)
            .or_insert_with(|| mirror.spaces.ensure(tx, &scene.spatial, slot))
    }

    fn clip(&mut self, tx: &mut Transaction, mirror: &mut Mirror, scene: &mut Scene, raw: u32) {
        self.clips.entry(raw).or_insert_with(|| {
            mirror.clips.ensure(
                tx,
                &mut scene.clips,
                &mut mirror.spaces,
                &scene.spatial,
                zgui_scene::ClipId(raw),
            )
        });
    }

    fn paint(
        &mut self,
        tx: &mut Transaction,
        mirror: &mut Mirror,
        scene: &mut Scene,
        reference: zgui_scene::PaintRef,
    ) {
        if let Some(id) = reference.id()
            && !self.paints.contains_key(&id.0)
        {
            let slot = mirror.paints.ensure(tx, &mut scene.paints, id);
            self.paints.insert(id.0, slot);
        }
    }
}

impl Recorded {
    /// Returns the reference to the coordinate system of `slot`.
    pub(crate) fn space(
        &mut self,
        refs: &Refs,
        chunk: &mut ChunkRecorder<'_>,
        slot: u32,
    ) -> SpaceRef {
        *self
            .spaces
            .entry(slot)
            .or_insert_with(|| chunk.space_ref(refs.spaces[&slot]))
    }

    /// Returns the reference to the clip chain `raw`, `NONE` for no clip.
    pub(crate) fn clip(&mut self, refs: &Refs, chunk: &mut ChunkRecorder<'_>, raw: u32) -> ClipRef {
        *self
            .clips
            .entry(raw)
            .or_insert_with(|| match refs.clips[&raw] {
                Some(id) => chunk.clip_ref(id),
                None => ClipRef::NONE,
            })
    }

    /// Returns the reference to the paint `reference` names, `NONE` for no paint or one the
    /// compositor has no slot for.
    pub(crate) fn paint(
        &mut self,
        refs: &Refs,
        chunk: &mut ChunkRecorder<'_>,
        reference: zgui_scene::PaintRef,
    ) -> PaintRef {
        let Some(id) = reference.id() else {
            return PaintRef::NONE;
        };
        *self
            .paints
            .entry(id.0)
            .or_insert_with(|| match refs.paints[&id.0] {
                Some((slot, kind)) => chunk.paint_ref(kind, slot),
                None => PaintRef::NONE,
            })
    }

    /// Returns the reference to the slot of the solid `color`.
    pub(crate) fn solid(
        &mut self,
        refs: &Refs,
        chunk: &mut ChunkRecorder<'_>,
        color: [f32; 4],
    ) -> PaintRef {
        let bits = color.map(f32::to_bits);
        *self
            .solids
            .entry(bits)
            .or_insert_with(|| chunk.paint_ref(PaintKind::SOLID, refs.solids[&bits]))
    }
}
