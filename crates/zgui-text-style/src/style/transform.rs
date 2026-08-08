//! `text-transform`: the property that changes which characters are shaped at all.
//!
//! Three independent decisions in one value, which is what the CSS grammar says: a case transform,
//! a width transform and a kana transform, the last two of which combine with the first and with
//! each other. They are kept apart here rather than flattened into one enumeration because that is
//! how they are applied — in that order, each to the output of the one before.

use crate::key::digest::Digest;

/// Which case a run's letters are put into.
///
/// Exclusive by the grammar: `uppercase`, `lowercase` and `capitalize` cannot be written together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CaseTransform {
    /// The text keeps the case the document wrote.
    #[default]
    None,
    /// Every letter is put into upper case.
    Upper,
    /// Every letter is put into lower case.
    Lower,
    /// The first letter of every word is put into title case, and the rest is left alone.
    Capitalize,
}

impl CaseTransform {
    /// Whether this changes nothing.
    pub const fn is_none(self) -> bool {
        matches!(self, Self::None)
    }
}

/// The whole of `text-transform`, as a run carries it.
///
/// ```
/// use zgui_text_style::{CaseTransform, TextTransform};
///
/// assert!(TextTransform::default().is_none());
/// assert!(!TextTransform { case: CaseTransform::Upper, ..Default::default() }.is_none());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextTransform {
    /// The case half.
    pub case: CaseTransform,
    /// `full-width`: narrow forms are replaced by their wide equivalents.
    pub full_width: bool,
    /// `full-size-kana`: small kana are replaced by their full-size equivalents.
    pub full_size_kana: bool,
}

impl TextTransform {
    /// The value a run with no `text-transform` carries.
    pub const fn none() -> Self {
        Self {
            case: CaseTransform::None,
            full_width: false,
            full_size_kana: false,
        }
    }

    /// Whether the transform leaves every character as the document wrote it.
    ///
    /// The fast path every run without the property takes, which is nearly all of them.
    pub const fn is_none(self) -> bool {
        self.case.is_none() && !self.full_width && !self.full_size_kana
    }

    /// Mixes this into a digest.
    pub fn hash_into(self, digest: &mut Digest) {
        digest.push(self.case);
        digest.push(self.full_width);
        digest.push(self.full_size_kana);
    }
}
