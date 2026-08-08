//! Upper, lower and title case, with the three languages that map letters differently.
//!
//! The default mappings are the standard library's, which are the *full* Unicode ones — one
//! character in, one or more out. What is added here is everything Unicode's `SpecialCasing` makes
//! conditional: the Greek final sigma, the Turkish and Azeri dotted and dotless `i`, and the
//! Lithuanian dot that survives above a lowercased letter under another accent.
//!
//! Title case is separate from upper case rather than derived from it. For most letters the two
//! agree; for the six Latin digraphs and for the Greek letters with iota subscript they do not, and
//! `text-transform: capitalize` asks for the title-case one.

/// Which language's tailoring applies.
///
/// Three of them exist, and everything else takes the untailored mapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    /// No tailoring: the mapping Unicode gives with no language condition.
    #[default]
    Default,
    /// Turkish or Azeri, which keep the dotted and dotless `i` apart in both directions.
    Turkic,
    /// Lithuanian, which keeps a dot above a lowercased letter that carries another accent.
    Lithuanian,
}

impl Language {
    /// The tailoring one BCP 47 language tag asks for.
    ///
    /// Only the primary subtag decides it, so `tr`, `tr-TR` and `TR` are one answer. An absent tag
    /// is the untailored mapping, which is what a document that declared no language gets.
    pub fn of(tag: Option<&str>) -> Self {
        let Some(tag) = tag else {
            return Self::Default;
        };
        let primary = tag.split(['-', '_']).next().unwrap_or(tag);
        if primary.eq_ignore_ascii_case("tr") || primary.eq_ignore_ascii_case("az") {
            Self::Turkic
        } else if primary.eq_ignore_ascii_case("lt") {
            Self::Lithuanian
        } else {
            Self::Default
        }
    }
}

/// `Σ`, the letter whose lower case depends on what follows it.
pub const SIGMA_CAPITAL: char = '\u{03A3}';
/// `ς`, the form a sigma takes at the end of a word.
pub const SIGMA_FINAL: &str = "\u{03C2}";
/// `σ`, the form it takes anywhere else.
pub const SIGMA_MEDIAL: &str = "\u{03C3}";
/// How many bytes either sigma form occupies, which is the same for both.
pub const SIGMA_LEN: usize = 2;

/// U+0307 COMBINING DOT ABOVE, which the Lithuanian and Turkic rules add and remove.
const DOT_ABOVE: char = '\u{0307}';

/// Appends the upper case of `character`.
pub fn upper(character: char, language: Language, out: &mut String) {
    if language == Language::Turkic {
        match character {
            'i' => {
                out.push('\u{0130}');
                return;
            }
            'ı' => {
                out.push('I');
                return;
            }
            _ => {}
        }
    }
    if language == Language::Lithuanian && character == DOT_ABOVE {
        // Lithuanian writes the dot above a lowercase `i` that carries another accent, and upper
        // case takes it away again: the capital `I` has no dot to keep. The dot is dropped wherever
        // it is met rather than only after a soft-dotted letter, because a combining dot above a
        // letter that had none is not something the transform put there.
        return;
    }
    out.extend(character.to_uppercase());
}

/// Appends the lower case of `character`.
///
/// The final sigma is *not* decided here — it depends on what comes after, which this cannot see —
/// so a caller that wants it handles the sigma itself and calls this for everything else.
pub fn lower(character: char, language: Language, out: &mut String) {
    if language == Language::Turkic {
        match character {
            'I' => {
                out.push('ı');
                return;
            }
            '\u{0130}' => {
                out.push('i');
                return;
            }
            _ => {}
        }
    }
    if language == Language::Lithuanian {
        match character {
            'I' => {
                out.push('i');
                out.push(DOT_ABOVE);
                return;
            }
            'J' => {
                out.push('j');
                out.push(DOT_ABOVE);
                return;
            }
            '\u{012E}' => {
                out.push('\u{012F}');
                out.push(DOT_ABOVE);
                return;
            }
            '\u{00CC}' => {
                out.push_str("i\u{0307}\u{0300}");
                return;
            }
            '\u{00CD}' => {
                out.push_str("i\u{0307}\u{0301}");
                return;
            }
            '\u{0128}' => {
                out.push_str("i\u{0307}\u{0303}");
                return;
            }
            _ => {}
        }
    }
    out.extend(character.to_lowercase());
}

/// Appends the title case of `character`.
///
/// Title case is upper case for all but a few dozen letters; those are listed rather than derived,
/// because there is no rule connecting a digraph to its title form.
pub fn title(character: char, language: Language, out: &mut String) {
    if let Some(titled) = titlecase(character) {
        out.push(titled);
        return;
    }
    upper(character, language, out);
}

/// The title case of one character, where it differs from the upper case.
fn titlecase(character: char) -> Option<char> {
    let code = character as u32;
    let titled = match code {
        // The three Latin digraphs, each with an all-caps, a title and a lowercase spelling.
        0x01C4..=0x01C6 => 0x01C5,
        0x01C7..=0x01C9 => 0x01C8,
        0x01CA..=0x01CC => 0x01CB,
        0x01F1..=0x01F3 => 0x01F2,
        // Greek letters with an iota subscript: the title form keeps the subscript, while the upper
        // form writes the iota out as a capital beside the letter.
        0x1F00..=0x1F0F | 0x1F20..=0x1F2F | 0x1F60..=0x1F6F => return None,
        0x1F80..=0x1F87 | 0x1F90..=0x1F97 | 0x1FA0..=0x1FA7 => code + 8,
        0x1F88..=0x1F8F | 0x1F98..=0x1F9F | 0x1FA8..=0x1FAF => code,
        0x1FB3 | 0x1FBC => 0x1FBC,
        0x1FC3 | 0x1FCC => 0x1FCC,
        0x1FF3 | 0x1FFC => 0x1FFC,
        _ => return None,
    };
    char::from_u32(titled)
}

/// Whether a character has a case at all, which is what the Final_Sigma rule looks either side for.
///
/// The Unicode `Cased` property. Approximated by the standard library's own answer — a character is
/// cased when it is upper case, lower case, or changes under either mapping — which agrees with the
/// property for every letter and differs only for a handful of symbols nothing writes a sigma
/// beside.
pub fn is_cased(character: char) -> bool {
    character.is_uppercase() || character.is_lowercase() || {
        let mut upper = character.to_uppercase();
        let mut lower = character.to_lowercase();
        upper.next() != Some(character)
            || upper.next().is_some()
            || lower.next() != Some(character)
            || lower.next().is_some()
    }
}

/// Whether a character is skipped when looking for the cased letter either side of a sigma.
///
/// The Unicode `Case_Ignorable` property: the marks, modifiers and the punctuation used inside
/// words. An apostrophe is here, which is what makes the sigma in `ΟΔΟΣ'` word-final and the `t` in
/// `don't` part of the same word.
pub fn is_case_ignorable(character: char) -> bool {
    matches!(
        character,
        '\'' | '\u{2019}' | '.' | '\u{00AD}' | ':' | '\u{02D7}' | '\u{02DE}',
    ) || matches!(
        character as u32,
        // Combining marks, which never break a word.
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0670
            | 0x06D6..=0x06DC
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0x20D0..=0x20F0
            | 0xFE00..=0xFE0F
            | 0xFE20..=0xFE2F
            // Spacing modifier letters and the modifier tone letters.
            | 0x02B0..=0x02C1
            | 0x02C6..=0x02D1
            | 0x02E0..=0x02E4
            | 0x02EC
            | 0x02EE
            | 0xA700..=0xA707
    )
}

/// Whether a character is a typographic letter unit, which is what a word starts with.
///
/// `capitalize` puts *the first letter of each word* into title case, and a word may begin with
/// something that is not a letter — a quotation mark, an opening bracket, a digit. The letter is
/// what takes the case, and everything before it neither takes one nor uses one up.
pub fn is_letter_unit(character: char) -> bool {
    character.is_alphabetic() || character.is_numeric()
}

#[cfg(test)]
mod tests {
    use super::{Language, is_case_ignorable, is_cased, is_letter_unit, lower, title, upper};

    /// One character's mapping, as a string.
    fn map(f: impl Fn(char, Language, &mut String), character: char, language: Language) -> String {
        let mut out = String::new();
        f(character, language, &mut out);
        out
    }

    /// A tag is read down to its primary subtag and no further.
    #[test]
    fn a_language_tag_is_matched_on_its_primary_subtag() {
        assert_eq!(Language::of(Some("tr")), Language::Turkic);
        assert_eq!(Language::of(Some("TR-tr")), Language::Turkic);
        assert_eq!(Language::of(Some("az-Latn-AZ")), Language::Turkic);
        assert_eq!(Language::of(Some("lt")), Language::Lithuanian);
        assert_eq!(Language::of(Some("en-GB")), Language::Default);
        assert_eq!(Language::of(None), Language::Default);
        // The tag is a prefix of a tailored one and is not that language.
        assert_eq!(Language::of(Some("trv")), Language::Default);
    }

    /// Lithuanian keeps a dot above a lowercased letter and takes it away again.
    #[test]
    fn lithuanian_keeps_the_dot_over_a_lowercased_letter() {
        assert_eq!(map(lower, 'I', Language::Lithuanian), "i\u{0307}");
        assert_eq!(map(lower, 'I', Language::Default), "i");
        assert_eq!(map(upper, '\u{0307}', Language::Lithuanian), "");
        assert_eq!(map(upper, '\u{0307}', Language::Default), "\u{0307}");
    }

    /// Title case and upper case differ for the digraphs and agree everywhere else.
    #[test]
    fn title_case_is_not_upper_case_applied_to_one_letter() {
        assert_eq!(map(title, 'ǆ', Language::Default), "ǅ");
        assert_eq!(map(upper, 'ǆ', Language::Default), "Ǆ");
        assert_eq!(map(title, '\u{1F80}', Language::Default), "\u{1F88}");
        assert_eq!(map(title, 'a', Language::Default), "A");
        assert_eq!(map(title, 'ß', Language::Default), "SS");
    }

    /// The three predicates answer what the sigma and word rules ask of them.
    #[test]
    fn the_predicates_separate_letters_marks_and_boundaries() {
        assert!(is_cased('a') && is_cased('Σ') && !is_cased('1') && !is_cased(' '));
        assert!(is_case_ignorable('\'') && is_case_ignorable('\u{0301}'));
        assert!(!is_case_ignorable(' ') && !is_case_ignorable('a'));
        assert!(is_letter_unit('a') && is_letter_unit('1') && is_letter_unit('あ'));
        assert!(!is_letter_unit(' ') && !is_letter_unit('('));
    }
}
