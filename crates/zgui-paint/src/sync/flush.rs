//! A fragment's items recorded as one compositor chunk.

use compositor_types::{ChunkFlags, ChunkKey, ChunkProps, ItemKind, Transaction};
use zgui_scene::Scene;

use crate::emit::record::{Items, Ref};
use crate::sync::mirror::Mirror;
use crate::sync::resolve::Resolver;

/// Records `items` under `key`, replacing the whole chunk.
///
/// The chunk's default space and clip are mirrored first.
pub(crate) fn record(
    tx: &mut Transaction,
    mirror: &mut Mirror,
    scene: &mut Scene,
    key: ChunkKey,
    items: &Items,
) {
    let mut resolver = Resolver::new(tx, mirror);
    let space = resolver.space(scene, items.space.index());
    let clip = if items.clip.is_root() {
        None
    } else {
        resolver.clip(scene, items.clip)
    };

    let mut chunk = tx.record(
        key,
        ChunkProps {
            space,
            clip,
            flags: ChunkFlags::default(),
        },
    );
    for reference in &items.refs {
        match *reference {
            Ref::Space(id) => {
                chunk.space_ref(id);
            }
            Ref::Clip(id) => {
                chunk.clip_ref(id);
            }
            Ref::Paint(kind, id) => {
                chunk.paint_ref(kind, id);
            }
        }
    }

    // A recording starts with empty arenas, so the ranges and local clip indices the items hold
    // stay what they were when the items were built.
    let glyphs = chunk.glyphs(&items.glyphs);
    debug_assert_eq!(glyphs.start, 0);
    let coords = chunk.coords(&items.coords);
    debug_assert_eq!(coords.start, 0);
    for clip in &items.local_clips {
        chunk.local_clip(clip.clone());
    }

    for op in &items.ops {
        let index = op.index as usize;
        match op.kind {
            ItemKind::QUAD => chunk.quad(items.quads[index]),
            ItemKind::SHADOW => chunk.shadow(items.shadows[index]),
            ItemKind::LINE => chunk.line(items.lines[index]),
            ItemKind::GLYPH_RUN => chunk.glyph_run(items.glyph_runs[index]),
            kind => unreachable!("the recorder holds no item of kind {kind:?}"),
        };
    }
}
