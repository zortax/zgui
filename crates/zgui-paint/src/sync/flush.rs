//! A fragment's items recorded as one compositor chunk.

use std::sync::Arc;

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
            Ref::Image(id) => {
                chunk.image_slot(id);
            }
        }
    }

    // A recording starts with empty arenas and lists, so the ranges and indices the items hold
    // stay what they were when the items were built.
    let glyphs = chunk.glyphs(&items.glyphs);
    debug_assert_eq!(glyphs.start, 0);
    let coords = chunk.coords(&items.coords);
    debug_assert_eq!(coords.start, 0);
    for clip in &items.local_clips {
        chunk.local_clip(clip.clone());
    }
    for payload in &items.payloads {
        chunk.payload(Arc::clone(payload));
    }
    for chain in &items.filters {
        chunk.filters(Arc::clone(chain));
    }

    for op in &items.ops {
        let index = op.index as usize;
        match op.kind {
            ItemKind::QUAD => chunk.quad(items.quads[index]),
            ItemKind::SHADOW => chunk.shadow(items.shadows[index]),
            ItemKind::LINE => chunk.line(items.lines[index]),
            ItemKind::GLYPH_RUN => chunk.glyph_run(items.glyph_runs[index]),
            ItemKind::IMAGE => chunk.image(items.images[index]),
            ItemKind::MARKS => chunk.marks(items.marks[index]),
            ItemKind::PATH => chunk.path(items.paths[index].clone()),
            ItemKind::BACKDROP => chunk.backdrop(items.backdrops[index]),
            kind => unreachable!("the recorder holds no item of kind {kind:?}"),
        };
    }
}
