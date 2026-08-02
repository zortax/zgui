//! Turning a face query into the terms a font collection understands.

use fontique::{Attributes, FontStyle, FontWeight, FontWidth, GenericFamily as FontiqueGeneric};
use smallvec::SmallVec;
use zgui_text::FaceQuery;
use zgui_text_style::{FamilyName, FontSlant, GenericFamily};

/// One entry of a family list, in the collection's own vocabulary.
///
/// Owned rather than borrowed because a query runs behind the collection's lock and the style it
/// came from is not held across it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum QueryEntry {
    /// A family named outright.
    Named(String),
    /// A role the environment resolves.
    Generic(FontiqueGeneric),
}

/// The family list a query walks, in author order.
pub(crate) fn families(query: &FaceQuery<'_>) -> SmallVec<[QueryEntry; 4]> {
    query
        .family
        .entries()
        .iter()
        .map(|entry| match entry {
            FamilyName::Named(name) => QueryEntry::Named(name.as_str().to_owned()),
            FamilyName::Generic(generic) => QueryEntry::Generic(generic_family(*generic)),
        })
        .collect()
}

/// The attributes a face is matched on.
pub(crate) fn attributes(query: &FaceQuery<'_>) -> Attributes {
    Attributes::new(
        FontWidth::from_ratio(query.width),
        slant(query.slant),
        FontWeight::new(query.weight),
    )
}

/// The collection's spelling of a slant.
pub(crate) fn slant(slant: FontSlant) -> FontStyle {
    match slant {
        FontSlant::Upright => FontStyle::Normal,
        FontSlant::Italic => FontStyle::Italic,
        FontSlant::Oblique(degrees) => FontStyle::Oblique(Some(degrees)),
    }
}

/// This framework's spelling of a slant.
pub(crate) fn slant_of(style: FontStyle) -> FontSlant {
    match style {
        FontStyle::Normal => FontSlant::Upright,
        FontStyle::Italic => FontSlant::Italic,
        FontStyle::Oblique(degrees) => {
            FontSlant::Oblique(degrees.unwrap_or(zgui_text_style::DEFAULT_OBLIQUE_DEGREES))
        }
    }
}

/// The collection's spelling of a generic family.
pub(crate) fn generic_family(generic: GenericFamily) -> FontiqueGeneric {
    match generic {
        GenericFamily::Serif => FontiqueGeneric::Serif,
        GenericFamily::SansSerif => FontiqueGeneric::SansSerif,
        GenericFamily::Monospace => FontiqueGeneric::Monospace,
        GenericFamily::Cursive => FontiqueGeneric::Cursive,
        GenericFamily::Fantasy => FontiqueGeneric::Fantasy,
        GenericFamily::SystemUi => FontiqueGeneric::SystemUi,
    }
}

/// Every generic role, so that registration can find the ones nothing is bound to yet.
pub(crate) const EVERY_GENERIC: [GenericFamily; 6] = [
    GenericFamily::Serif,
    GenericFamily::SansSerif,
    GenericFamily::Monospace,
    GenericFamily::Cursive,
    GenericFamily::Fantasy,
    GenericFamily::SystemUi,
];
