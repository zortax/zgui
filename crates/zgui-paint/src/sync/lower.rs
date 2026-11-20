//! The scene quads the vector emitters push, merged into a chunk's items.

use compositor_types::{BorderStyle, Quad};
use zgui_scene::{ChunkPrims, ClipId, PrimitiveKind, Scene};

use crate::emit::record::{Items, Recorder};
use crate::sync::resolve::Resolver;

/// Reports whether `prims` holds a quad.
pub(crate) fn holds_quads(prims: &ChunkPrims) -> bool {
    !prims.quads.is_empty()
}

/// Fills `out` with `items` and the quads of `prims`, in the order the fragment drew them.
///
/// The other primitives of `prims` are left out.
pub(crate) fn merge(
    items: &Items,
    prims: &ChunkPrims,
    scene: &mut Scene,
    resolver: Resolver<'_>,
    out: &mut Items,
) {
    let mut rec = Recorder::new(scene, out, Some(resolver), items.space, items.clip);
    let merged = rec.items_mut();
    merged.glyphs.clone_from(&items.glyphs);
    merged.local_clips.clone_from(&items.local_clips);
    merged.refs.clone_from(&items.refs);
    merged.instanced = items.instanced;

    let mut next = 0;
    for (op, &at) in items.ops.iter().zip(&items.at) {
        while next < at as usize {
            quad(&mut rec, prims, next);
            next += 1;
        }
        rec.items_mut().push_from(items, *op, 0);
    }
    for index in next..prims.ops.len() {
        quad(&mut rec, prims, index);
    }
}

/// Records the operation `index` of `prims` if it is a quad.
fn quad(rec: &mut Recorder<'_>, prims: &ChunkPrims, index: usize) {
    let op = prims.ops[index];
    if op.kind != PrimitiveKind::Quad {
        return;
    }
    let quad = &prims.quads[op.index as usize];
    let item = Quad {
        bounds: quad.bounds,
        radii: quad.radii,
        shapes: [quad.shape; 4],
        border: quad.border,
        fill: rec.paint(quad.fill),
        stroke: rec.paint(quad.stroke),
        paint_origin: quad.paint_origin,
        clip: rec.clip(ClipId(quad.clip)),
        space: rec.space_slot(quad.transform),
        // The low byte holds the border style, and both vocabularies number it alike.
        style: BorderStyle((quad.style & 0xff) as u8),
        _pad: [0; 3],
    };
    rec.quad(item);
}
