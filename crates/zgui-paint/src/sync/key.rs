//! The chunk keys fragments are recorded under.

use compositor_types::ChunkKey;
use zgui_layout::{FragKey, Fragment, LayoutStore};

/// The part of a fragment's painting a chunk holds, in the low bits of the key's part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// Everything but the outline.
    Body = 0,
}

/// Returns the key of `part` of `fragment`.
///
/// The owner is the fragment's box and the part is the fragment's position among the box's
/// fragments, so a fragment rebuilt in its box's place keeps its key.
pub(crate) fn of(store: &LayoutStore, fragment: &Fragment, part: Part) -> ChunkKey {
    ChunkKey::new(
        fragment.box_.as_u64(),
        ordinal(store, fragment) << 4 | part as u32,
    )
}

/// Returns the key of `part` of the fragment at `ordinal` among the fragments of the box `owner`.
pub(crate) fn at(owner: u64, ordinal: u32, part: Part) -> ChunkKey {
    ChunkKey::new(owner, ordinal << 4 | part as u32)
}

/// Returns the position of `fragment` among its box's fragments.
fn ordinal(store: &LayoutStore, fragment: &Fragment) -> u32 {
    position(store.fragments_of_box(fragment.box_), fragment.key)
}

/// Returns the position of `key` in `fragments`, zero when it is not there.
fn position(fragments: &[FragKey], key: FragKey) -> u32 {
    fragments
        .iter()
        .position(|candidate| *candidate == key)
        .map_or(0, |at| at as u32)
}
