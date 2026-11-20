//! The font system as the compositor's font registry.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, PoisonError};

use compositor_types::{FontData, FontId};
use rustc_hash::{FxHashMap, FxHasher};
use zgui_text::{FaceId, FontSource};

use crate::system::FontSystem;

/// A [`FontSystem`]'s faces as a compositor font registry.
///
/// A face's font id is its face id, so glyph runs name the faces layout shaped them with. A face
/// shares its font file with the font system, without a copy.
pub struct CompositorFonts {
    system: Arc<FontSystem>,
    /// The fonts registered through the registry, by content.
    by_content: Mutex<FxHashMap<(u64, usize, u32), FontId>>,
}

impl CompositorFonts {
    /// Returns the registry of `system`'s faces.
    pub fn new(system: Arc<FontSystem>) -> Self {
        Self {
            system,
            by_content: Mutex::new(FxHashMap::default()),
        }
    }
}

impl compositor_api::FontRegistry for CompositorFonts {
    fn register(&self, data: FontData) -> FontId {
        let content = (fingerprint(data.bytes()), data.bytes().len(), data.index);
        let mut by_content = self
            .by_content
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(id) = by_content.get(&content) {
            return *id;
        }
        let faces = self
            .system
            .register(Arc::clone(&data.file), None)
            .unwrap_or_default();
        let id = faces
            .iter()
            .copied()
            .find(|face| self.index_of(*face) == Some(data.index))
            .or_else(|| faces.first().copied())
            // A file with no face gets an id no face has, which `get` answers with nothing.
            .map_or(FontId(u32::MAX), |face| FontId(face.0));
        by_content.insert(content, id);
        id
    }

    fn get(&self, id: FontId) -> Option<FontData> {
        self.system.locked(|shared| {
            let entry = shared.faces.get(FaceId(id.0))?;
            let (file, _) = entry.blob.clone().into_raw_parts();
            Some(FontData {
                file,
                index: entry.index,
            })
        })
    }

    fn generation(&self) -> u64 {
        self.system.locked(|shared| shared.faces.len() as u64)
    }
}

impl CompositorFonts {
    /// Returns the index in its file of the face `face`.
    fn index_of(&self, face: FaceId) -> Option<u32> {
        self.system
            .locked(|shared| shared.faces.get(face).map(|entry| entry.index))
    }
}

impl core::fmt::Debug for CompositorFonts {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CompositorFonts").finish_non_exhaustive()
    }
}

/// Returns a hash of a font file's bytes.
fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = FxHasher::default();
    bytes.hash(&mut hasher);
    hasher.finish()
}
