//! A fragment's items recorded as one compositor chunk.

use crate::Tables;
use iceblink::{ChunkFlags, ChunkKey, ChunkProps, Transaction};

use crate::emit::record::Items;
use crate::sync::mirror::Mirror;
use crate::sync::resolve::Resolver;

/// Records `items` under `key`, replacing the whole chunk.
///
/// The chunk's default space and clip are mirrored first. The items move into the chunk, and
/// `items` is left empty.
pub(crate) fn record(
    tx: &mut Transaction,
    mirror: &mut Mirror,
    tables: &mut Tables,
    key: ChunkKey,
    items: &mut Items,
) {
    let mut resolver = Resolver::new(tx, mirror);
    let space = resolver.space(tables, items.space.index());
    let clip = if items.clip.is_root() {
        None
    } else {
        resolver.clip(tables, items.clip)
    };

    let mut chunk = tx.empty_chunk(
        key,
        ChunkProps {
            space,
            clip,
            flags: ChunkFlags::default(),
        },
    );
    items.move_into(&mut chunk);
    tx.record_chunk(chunk);
}
