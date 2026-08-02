//! Registration and face resolution.

use fontique::Blob;
use smallvec::SmallVec;
use zgui_interned::Ident;
use zgui_text::{FaceId, FaceRecord, FontData, FontError, FontSource};
use zgui_text_style::GenericFamily;

use crate::font::query::first_match;
use crate::font::register::register_into;
use crate::font::resolve::{self, QueryEntry};
use crate::system::{Enumeration, FontSystem};

impl FontSource for FontSystem {
    fn register(
        &self,
        data: FontData,
        family: Option<Ident>,
    ) -> Result<SmallVec<[FaceId; 4]>, FontError> {
        let blob = Blob::new(data);
        if blob.data().len() < 4 {
            return Err(FontError::Unrecognised);
        }
        let registered = self.locked(|shared| register_into(shared, blob, family));
        if registered.is_ok() {
            // A face that now wins a match the memo already answered would otherwise keep
            // answering with the face that used to win it, and two elements cascaded either side
            // of the registration would disagree about how tall an `ex` is.
            self.forget_metrics();
        }
        registered
    }

    fn unregister(&self, family: Ident) {
        self.locked(|shared| {
            let handles = shared.faces.forget_family(family);
            for handle in handles {
                let Some(entry) = shared.faces.get(handle) else {
                    continue;
                };
                let Some(family_id) = entry.family_id else {
                    continue;
                };
                let record = entry.record.clone();
                shared.collection.unregister_font(
                    family_id,
                    fontique::FontWidth::from_ratio(record.width),
                    resolve::slant(record.slant),
                    fontique::FontWeight::new(record.weight),
                );
            }
        });
        self.forget_metrics();
    }

    fn resolve(&self, query: &zgui_text::FaceQuery<'_>) -> Option<FaceId> {
        let families = resolve::families(query);
        let attributes = resolve::attributes(query);
        self.locked(|shared| first_match(shared, &families, attributes, None))
    }

    fn resolve_for(&self, query: &zgui_text::FaceQuery<'_>, character: char) -> Option<FaceId> {
        let mut families = resolve::families(query);
        let attributes = resolve::attributes(query);
        let sweep = self.options().enumeration == Enumeration::Registered;
        self.locked(|shared| {
            if let Some(found) = first_match(shared, &families, attributes, Some(character)) {
                return Some(found);
            }
            if !sweep {
                return None;
            }
            // A collection with no system behind it has no fallback list of its own, so the
            // registered families *are* the fallback list. Sweeping them is bounded by what the
            // application registered, which is why it is only done in this mode: over an
            // enumerated system it would be a walk of every face installed.
            let names: Vec<String> = shared
                .collection
                .family_names()
                .map(str::to_owned)
                .collect();
            families.extend(names.into_iter().map(QueryEntry::Named));
            first_match(shared, &families, attributes, Some(character))
        })
    }

    fn face(&self, id: FaceId) -> Option<FaceRecord> {
        self.locked(|shared| shared.faces.get(id).map(|entry| entry.record.clone()))
    }

    fn generic_family(&self, generic: GenericFamily) -> Option<Ident> {
        self.locked(|shared| {
            let role = resolve::generic_family(generic);
            let first = shared.collection.generic_families(role).next()?;
            shared.collection.family_name(first).map(Ident::new)
        })
    }
}
