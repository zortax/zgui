//! The face bytes a rasteriser needs, held apart from the collection.

use std::sync::RwLock;

use fontique::Blob;
use rustc_hash::FxHashMap;
use zgui_text::FaceId;

use crate::system::FontSystem;

/// The file a face lives in and its index within it, or the fact that no such face exists.
type Lookup = Option<(Blob<u8>, u32)>;

/// Face bytes already looked up, so that rasterising a glyph does not reach the collection.
///
/// Rasterisation happens while a frame is being built and the collection's lock is the same one
/// the cascade's metrics queries take. Looking a face's bytes up once and holding the shared blob
/// keeps the two apart: the first glyph of a face pays a lock acquisition, and no glyph after it
/// does.
///
/// # Why there is no invalidation
///
/// An entry here can never go stale, and that is a property of what a face handle *is* rather than
/// a bet about how the cache is used: a handle names a file and an index, both settled when the
/// handle was issued, and a handle is never withdrawn or reissued. Taking a family out of the font
/// system stops it being *found* by name; it does not change what a handle already handed out
/// resolves to, because a paragraph shaped before that happened is still on screen and still has to
/// be drawn.
#[derive(Debug, Default)]
pub(crate) struct FaceBytes {
    /// Handle to the file the face lives in and its index within it.
    entries: RwLock<FxHashMap<FaceId, Lookup>>,
}

impl FaceBytes {
    /// The bytes and index behind a handle, or nothing if the system never issued it.
    pub(crate) fn get(&self, fonts: &FontSystem, face: FaceId) -> Lookup {
        if let Some(found) = self
            .entries
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&face)
        {
            return found.clone();
        }
        let looked_up = fonts.locked(|shared| {
            shared
                .faces
                .get(face)
                .map(|entry| (entry.blob.clone(), entry.index))
        });
        self.entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(face, looked_up.clone());
        looked_up
    }
}
