//! Binding the generic family roles when nothing else will.

use fontique::FamilyId;

use crate::font::resolve::{EVERY_GENERIC, generic_family};
use crate::system::shared::Shared;

/// Binds `family` to every generic role that has nothing bound to it.
///
/// A collection built without system enumeration starts with no generic bindings at all, so
/// `font-family: sans-serif` — which is what an unstyled document resolves to — would find no face
/// and every font-relative unit in the document would take its fallback branch. Filling the empty
/// roles in from the first family registered is what makes a document with one registered face
/// behave like a document on a machine with fonts installed.
///
/// Roles that already have a family bound are left alone, so registering a second face does not
/// displace the first, and a caller that binds a role deliberately keeps its choice.
pub(crate) fn fill_empty_roles(shared: &mut Shared, family: FamilyId) {
    for generic in EVERY_GENERIC {
        let role = generic_family(generic);
        if shared.collection.generic_families(role).next().is_none() {
            shared
                .collection
                .set_generic_families(role, core::iter::once(family));
        }
    }
}
