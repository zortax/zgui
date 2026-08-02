//! The font-family list.

use smallvec::SmallVec;
use zgui_interned::Ident;

use crate::key::digest::Digest;

/// A family name that stands for whatever face the environment has configured for that role.
///
/// Which face each of these resolves to is a property of the system and of the document's
/// language, not of the style, so a style carries the role and the resolution happens where faces
/// are known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GenericFamily {
    /// `serif`.
    Serif,
    /// `sans-serif`.
    SansSerif,
    /// `monospace`.
    Monospace,
    /// `cursive`.
    Cursive,
    /// `fantasy`.
    Fantasy,
    /// `system-ui`.
    SystemUi,
}

/// One entry of a family list.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FamilyName {
    /// A family named outright, matched against the faces that are installed or registered.
    Named(Ident),
    /// A role the environment resolves.
    Generic(GenericFamily),
}

/// A `font-family` list, in the order the author wrote it.
///
/// Face selection walks it front to back and takes the first family that has a face covering the
/// character being shaped, so order is part of the value and never sorted.
///
/// ```
/// use zgui_text_style::{FamilyName, FontFamilyList, GenericFamily};
/// use zgui_interned::Ident;
///
/// let list = FontFamilyList::from_iter([
///     FamilyName::Named(Ident::new("Inter")),
///     FamilyName::Generic(GenericFamily::SansSerif),
/// ]);
/// assert_eq!(list.first_generic(), Some(GenericFamily::SansSerif));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontFamilyList {
    /// The entries, in author order.
    entries: SmallVec<[FamilyName; 4]>,
}

impl FontFamilyList {
    /// A list holding one generic family, which is what an unstyled document starts from.
    pub fn generic(family: GenericFamily) -> Self {
        Self::from_iter([FamilyName::Generic(family)])
    }

    /// The entries, in author order.
    pub fn entries(&self) -> &[FamilyName] {
        &self.entries
    }

    /// Whether the list is empty, which a cascaded style's never is.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The first generic in the list, which is the role a fallback resolves through.
    pub fn first_generic(&self) -> Option<GenericFamily> {
        self.entries.iter().find_map(|entry| match entry {
            FamilyName::Generic(family) => Some(*family),
            FamilyName::Named(_) => None,
        })
    }

    /// Mixes the list into a digest.
    pub(crate) fn hash_into(&self, digest: &mut Digest) {
        digest.push(self.entries.len());
        for entry in &self.entries {
            match entry {
                FamilyName::Named(name) => {
                    digest.push_tag(0);
                    digest.push(name.as_str());
                }
                FamilyName::Generic(family) => {
                    digest.push_tag(1);
                    digest.push(*family);
                }
            }
        }
    }
}

impl FromIterator<FamilyName> for FontFamilyList {
    fn from_iter<I: IntoIterator<Item = FamilyName>>(entries: I) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }
}
