//! The table of faces the collection has handed out handles for.

use fontique::{Blob, FamilyId};
use rustc_hash::FxHashMap;
use zgui_interned::Ident;
use zgui_text::{FaceId, FaceRecord};

/// One face, as this crate needs it: the record a caller sees plus the bytes it lives in.
#[derive(Clone, Debug)]
pub(crate) struct FaceEntry {
    /// What a caller is told about the face.
    pub(crate) record: FaceRecord,
    /// The family the collection filed it under, when the face was reached through one.
    ///
    /// Absent for a face the shaper reached through fallback: it was never registered under a
    /// name here, so there is no registration for it to be removed from either.
    pub(crate) family_id: Option<FamilyId>,
    /// The file the face lives in.
    pub(crate) blob: Blob<u8>,
    /// The face's index within that file.
    pub(crate) index: u32,
}

/// Every face this system has issued a handle for, and the way back from one.
#[derive(Clone, Debug, Default)]
pub(crate) struct FaceTable {
    /// Indexed by [`FaceId`].
    entries: Vec<FaceEntry>,
    /// Blob identity and face index to the handle already issued for it.
    by_blob: FxHashMap<(u64, u32), FaceId>,
    /// Which handles each registered family owns, so unregistering can find them.
    by_family: FxHashMap<Ident, Vec<FaceId>>,
}

impl FaceTable {
    /// The handle for a face, issuing one if this face has not been seen before.
    ///
    /// Idempotent by blob identity, so registering the same file twice under two family names
    /// leaves one handle and not two — which is what keeps a glyph cache from holding the same
    /// pixels twice.
    pub(crate) fn intern(
        &mut self,
        family: Ident,
        family_id: Option<FamilyId>,
        blob: Blob<u8>,
        index: u32,
        describe: impl FnOnce(FaceId) -> FaceRecord,
    ) -> FaceId {
        let key = (blob.id(), index);
        if let Some(existing) = self.by_blob.get(&key) {
            return *existing;
        }
        let id = FaceId(self.entries.len() as u32);
        self.entries.push(FaceEntry {
            record: describe(id),
            family_id,
            blob,
            index,
        });
        self.by_blob.insert(key, id);
        self.by_family.entry(family).or_default().push(id);
        id
    }

    /// The entry behind a handle.
    pub(crate) fn get(&self, id: FaceId) -> Option<&FaceEntry> {
        self.entries.get(id.0 as usize)
    }

    /// Forgets that a family was registered.
    ///
    /// The entries themselves stay, because a handle already handed out must keep resolving: a
    /// paragraph shaped before a sheet dropped its `@font-face` rule is still on screen.
    pub(crate) fn forget_family(&mut self, family: Ident) -> Vec<FaceId> {
        self.by_family.remove(&family).unwrap_or_default()
    }
}
