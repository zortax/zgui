//! What a face is looked up by.

use zgui_interned::Ident;
use zgui_text_style::{FontFamilyList, FontSlant, FontVariation, TextStyle};

/// The part of a run's style that selects a face.
///
/// Deliberately a borrow rather than an owned value: face lookup happens on a hot path, once per
/// run per restyle, and a query that cloned the family list would allocate on every one of those.
///
/// It is also deliberately *less* than the whole style. Everything here changes which face is
/// chosen; nothing here is a property that only changes what is done with the face afterwards, so
/// two runs with equal queries resolve to the same face however else they differ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceQuery<'a> {
    /// The families to try, in author order.
    pub family: &'a FontFamilyList,
    /// The weight to match, between 1 and 1000.
    pub weight: f32,
    /// The slant to match.
    pub slant: FontSlant,
    /// The width to match, as a fraction of normal.
    pub width: f32,
    /// The variable-axis coordinates to instance the face at, in author order.
    pub variations: &'a [FontVariation],
    /// The language the text is in, which selects a face's locale-specific forms.
    pub language: Option<Ident>,
}

impl<'a> FaceQuery<'a> {
    /// The query one run's style makes.
    pub fn of(style: &'a TextStyle) -> Self {
        Self {
            family: &style.family,
            weight: style.weight,
            slant: style.slant,
            width: style.width,
            variations: &style.variations,
            language: style.language,
        }
    }
}
