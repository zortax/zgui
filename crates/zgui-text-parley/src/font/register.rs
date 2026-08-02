//! Adding a font file's faces to the collection.

use fontique::{Blob, FontInfo, FontInfoOverride};
use smallvec::SmallVec;
use zgui_interned::Ident;
use zgui_text::{FaceId, FaceRecord, FontError};

use crate::font::resolve;
use crate::system::generics::fill_empty_roles;
use crate::system::shared::Shared;

/// Adds every face in one file to the collection and issues handles for them.
pub(crate) fn register_into(
    shared: &mut Shared,
    blob: Blob<u8>,
    family: Option<Ident>,
) -> Result<SmallVec<[FaceId; 4]>, FontError> {
    let overrides = family.map(|name| FontInfoOverride {
        family_name: Some(name.as_str()),
        ..FontInfoOverride::default()
    });
    let registered = shared.collection.register_fonts(blob, overrides);
    if registered.is_empty() {
        return Err(FontError::Unrecognised);
    }
    let mut handles = SmallVec::new();
    for (family_id, fonts) in registered {
        let name = shared
            .collection
            .family_name(family_id)
            .map_or_else(|| Ident::new(""), Ident::new);
        if fonts.is_empty() {
            return Err(FontError::Empty);
        }
        for info in fonts {
            handles.push(intern_face(shared, name, family_id, &info)?);
        }
        fill_empty_roles(shared, family_id);
    }
    Ok(handles)
}

/// Issues one face's handle, reading the colour tables once per file.
fn intern_face(
    shared: &mut Shared,
    family: Ident,
    family_id: fontique::FamilyId,
    info: &FontInfo,
) -> Result<FaceId, FontError> {
    let blob = info
        .load(Some(&mut shared.sources))
        .ok_or(FontError::Malformed("the face's bytes could not be read"))?;
    let index = info.index();
    let colors = shared.color_support((blob.id(), index), blob.data());
    let variable = !info.axes().is_empty();
    let (weight, slant, width) = (
        info.weight().value(),
        resolve::slant_of(info.style()),
        info.width().ratio(),
    );
    Ok(shared
        .faces
        .intern(family, Some(family_id), blob, index, |id| FaceRecord {
            id,
            family,
            weight,
            slant,
            width,
            is_variable: variable,
            has_color: colors.any(),
        }))
}
