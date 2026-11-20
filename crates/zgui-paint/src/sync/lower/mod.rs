//! One fragment's captured painting recorded as a compositor chunk.

mod items;
mod refs;

use compositor_types::{ChunkFlags, ChunkKey, ChunkProps, Glyph, Transaction};
use zgui_scene::{ChunkPrims, PrimitiveKind, Scene, SpatialId};

use crate::sync::mirror::Mirror;

/// What recording one chunk left out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Omitted {
    /// Primitives of a kind the chunk lowering does not record yet.
    pub(crate) primitives: usize,
    /// Glyph runs drawn at a variation instance, recorded at the default instance.
    pub(crate) instances: usize,
}

/// Records `chunk` under `key`, replacing the whole compositor chunk.
///
/// Every node and slot the chunk refers to is mirrored first. Quads, shadows, decoration lines
/// and glyph runs are recorded in the order the encoding pushed them. Sprites that draw a captured
/// glyph run are covered by the run.
pub(crate) fn record(
    tx: &mut Transaction,
    mirror: &mut Mirror,
    scene: &mut Scene,
    key: ChunkKey,
    chunk: &ChunkPrims,
) -> Omitted {
    let mut refs = refs::Refs::default();
    refs.gather(tx, mirror, scene, chunk);
    let space = refs.space(tx, mirror, scene, SpatialId::VIEWPORT.index());

    let mut omitted = Omitted::default();
    let mut recorded = refs::Recorded::default();
    let mut glyphs: Vec<Glyph> = Vec::new();
    let mut runs = chunk.glyph_runs.iter().peekable();
    let mut recorder = tx.record(
        key,
        ChunkProps {
            space,
            clip: None,
            flags: ChunkFlags::default(),
        },
    );

    for (at, op) in chunk.ops.iter().enumerate() {
        while let Some(run) = runs.next_if(|run| run.at as usize <= at) {
            let instanced = items::glyph_run(&mut recorder, &refs, &mut recorded, run, &mut glyphs);
            omitted.instances += usize::from(instanced);
        }
        let index = op.index as usize;
        match op.kind {
            PrimitiveKind::Quad => {
                items::quad(&mut recorder, &refs, &mut recorded, &chunk.quads[index]);
            }
            PrimitiveKind::Shadow => {
                items::shadow(&mut recorder, &refs, &mut recorded, &chunk.shadows[index]);
            }
            PrimitiveKind::Decoration => {
                items::line(
                    &mut recorder,
                    &refs,
                    &mut recorded,
                    &chunk.decorations[index],
                );
            }
            PrimitiveKind::MonoSprite
            | PrimitiveKind::SubpixelSprite
            | PrimitiveKind::ColorSprite
                if !chunk.glyph_runs.is_empty() => {}
            _ => omitted.primitives += 1,
        }
    }
    for run in runs {
        let instanced = items::glyph_run(&mut recorder, &refs, &mut recorded, run, &mut glyphs);
        omitted.instances += usize::from(instanced);
    }
    omitted
}
