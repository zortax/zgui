//! The chunk keys fragments are recorded under.

use iceblink::ChunkKey;
use zgui_layout::{FragKey, Fragment, LayoutStore};

/// The part of a fragment's painting a chunk holds, in the low bits of the key's part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// Everything but the outline.
    Body = 0,
    /// The outline, drawn after the box's descendants.
    Outline = 1,
    /// The backdrop filter, drawn before the box's own painting, outside its group.
    Backdrop = 2,
}

/// Returns the key of `part` of `fragment`, whose box is recorded under `name`.
///
/// The part is the fragment's position among the box's fragments, so a fragment rebuilt in its
/// box's place keeps its key.
pub(crate) fn of(name: u64, store: &LayoutStore, fragment: &Fragment, part: Part) -> ChunkKey {
    ChunkKey::new(name, ordinal(store, fragment) << 4 | part as u32)
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
