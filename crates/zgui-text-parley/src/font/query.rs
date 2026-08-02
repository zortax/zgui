//! Finding the face a query resolves to.

use fontique::{Blob, QueryFamily, QueryStatus};
use zgui_interned::Ident;
use zgui_text::{FaceId, FaceRecord};

use crate::font::resolve::{self, QueryEntry};
use crate::font::script::script_of;
use crate::system::shared::Shared;

/// The first face in `families` that matches, optionally one that can draw `character`.
pub(crate) fn first_match(
    shared: &mut Shared,
    families: &[QueryEntry],
    attributes: fontique::Attributes,
    character: Option<char>,
) -> Option<FaceId> {
    let mut found: Option<(fontique::FamilyId, Blob<u8>, u32)> = None;
    {
        let Shared {
            collection,
            sources,
            ..
        } = shared;
        let mut query = collection.query(sources);
        query.set_families(families.iter().map(|entry| match entry {
            QueryEntry::Named(name) => QueryFamily::Named(name.as_str()),
            QueryEntry::Generic(generic) => QueryFamily::Generic(*generic),
        }));
        query.set_attributes(attributes);
        if let Some(character) = character {
            query.set_fallbacks(fontique::FallbackKey::new(script_of(character), None));
        }
        query.matches_with(|font| {
            if let Some(character) = character
                && font
                    .charmap()
                    .is_none_or(|charmap| charmap.map(character).is_none())
            {
                return QueryStatus::Continue;
            }
            found = Some((font.family.0, font.blob.clone(), font.index));
            QueryStatus::Stop
        });
    }
    let (family_id, blob, index) = found?;
    let name = shared
        .collection
        .family_name(family_id)
        .map_or_else(|| Ident::new(""), Ident::new);
    let own = shared.collection.family(family_id).and_then(|family| {
        family
            .fonts()
            .iter()
            .find(|font| font.index() == index)
            .cloned()
    });
    let colors = shared.color_support((blob.id(), index), blob.data());
    // The axis values reported are the face's own and not the ones asked for, which is what tells
    // a caller that a weight it wanted is going to have to be synthesised.
    let weight = own.as_ref().map_or(400.0, |font| font.weight().value());
    let slant = own
        .as_ref()
        .map_or(zgui_text_style::FontSlant::Upright, |font| {
            resolve::slant_of(font.style())
        });
    let width = own.as_ref().map_or(1.0, |font| font.width().ratio());
    let variable = own.as_ref().is_some_and(|font| !font.axes().is_empty());
    Some(
        shared
            .faces
            .intern(name, Some(family_id), blob, index, |id| FaceRecord {
                id,
                family: name,
                weight,
                slant,
                width,
                is_variable: variable,
                has_color: colors.any(),
            }),
    )
}
