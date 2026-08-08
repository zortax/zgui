//! `text-transform`: changing which characters are shaped, before anything is shaped.
//!
//! # Why this is a character-at-a-time interface rather than a string one
//!
//! The obvious shape for a case mapping is *string in, string out*, and it is the wrong one here.
//! What the caller is building is the transformed text **together with the map back to the
//! document's own bytes**, which is what a caret, a selection, a hit test and an accessible run all
//! resolve through. A case mapping changes lengths — `ß` becomes two bytes' worth of `SS`, `İ`
//! lowercases to three bytes — so the correspondence is knowable only while each character is being
//! converted, and a mapping recovered afterwards by comparing two strings is a second answer that
//! can disagree with the first.
//!
//! So [`Transformer::push`] takes one character and appends what it becomes, and the caller records
//! the range it produced. Everything context-dependent is carried in the transformer rather than
//! looked up in the output, which is what lets a word run across an inline box boundary and a Greek
//! sigma be decided by the letter that follows it.
//!
//! # What is implemented
//!
//! The Unicode *default* case conversion, which is what `char::to_uppercase` and
//! `char::to_lowercase` already are: the full mappings, so `ß` uppercases to `SS` and `ﬁ` to `FI`.
//! On top of that, every conditional mapping Unicode's `SpecialCasing` defines:
//!
//! * **Final_Sigma** — `Σ` lowercases to `ς` at the end of a word and to `σ` inside one;
//! * **Turkish and Azeri** — the dotted and dotless `i`, both directions;
//! * **Lithuanian** — the dot that is kept above a lowercased `i` under another accent, and removed
//!   again when it is uppercased.
//!
//! Title case is its own mapping rather than upper case applied to one letter, because for the
//! digraphs and for the Greek letters with iota subscript the two differ — `ǆ` title-cases to `ǅ`
//! and upper-cases to `Ǆ`, and `capitalize` asks for the first of those.
//!
//! | Module | What it maps |
//! |---|---|
//! | [`case`] | upper, lower and title case, with the language conditions |
//! | [`width`] | `full-width`: narrow and half-width forms to their wide equivalents |
//! | [`kana`] | `full-size-kana`: small kana to full-size kana |

pub mod case;
pub mod kana;
pub mod width;

use zgui_text_style::{CaseTransform, TextTransform};

pub use crate::transform::case::Language;

/// One run's transform, and the context a conditional mapping needs.
///
/// Held across a whole inline formatting context rather than per run: a word split by an inline box
/// boundary is one word, so `capitalize` must not capitalise its second half, and a sigma at the end
/// of one run is followed by whatever the next run starts with.
#[derive(Clone, Debug)]
pub struct Transformer {
    /// What to apply.
    transform: TextTransform,
    /// The language the conditional mappings are resolved for.
    language: Language,
    /// Whether the next letter starts a word, which is what `capitalize` acts on.
    word_start: bool,
    /// Whether the last character emitted was cased, which Final_Sigma reads.
    after_cased: bool,
    /// Where a provisional final sigma was written, if one is still provisional.
    ///
    /// A sigma is word-final until a cased letter follows it, and the letter that would settle the
    /// question has not been seen when the sigma is emitted. So the word-final form is written and
    /// the position kept: a cased letter arriving later overwrites those two bytes with the
    /// non-final form. Both forms are two bytes in UTF-8, so nothing after them moves and no map
    /// entry becomes wrong.
    pending_sigma: Option<usize>,
}

impl Transformer {
    /// A transformer for one run's style.
    pub fn new(transform: TextTransform, language: Option<&str>) -> Self {
        Self {
            transform,
            language: Language::of(language),
            word_start: true,
            after_cased: false,
            pending_sigma: None,
        }
    }

    /// Whether this changes nothing, so a caller may take its own fast path.
    pub fn is_identity(&self) -> bool {
        self.transform.is_none()
    }

    /// Puts a different run's transform in force, keeping the context built up so far.
    ///
    /// What is kept is exactly what an inline box boundary must not disturb: whether a word is in
    /// progress, whether the last letter was cased, and whether a sigma is still waiting to find out
    /// if it was word-final. A run with no transform of its own reconfigures to the identity and
    /// goes on maintaining all three, which is what lets the run after it be right.
    pub fn reconfigure(&mut self, transform: TextTransform, language: Option<&str>) {
        self.transform = transform;
        self.language = Language::of(language);
    }

    /// Resumes as if `previous` had just been emitted.
    ///
    /// Used when the first run that asks for a transform is not the first run in the paragraph: the
    /// text already generated is what decides whether the next letter starts a word, and reading it
    /// off is what stops `capitalize` from capitalising the second half of a split word.
    pub fn resume_after(&mut self, previous: Option<char>) {
        let Some(previous) = previous else {
            return;
        };
        self.advance_word_state(previous);
    }

    /// Appends what `character` becomes to `out`, in the context seen so far.
    ///
    /// `preserved` says whether the character is white space the style preserves, which is the one
    /// thing `full-width` needs to know: it maps a space to an ideographic space only where the
    /// space survives as a space, and a collapsed run of white space has become a single ordinary
    /// space that stands for the run rather than for a character of its own.
    ///
    /// `out` is the whole generated string, appended to in place, because the sigma fix-up rewrites
    /// bytes already in it.
    pub fn push(&mut self, character: char, preserved: bool, out: &mut String) {
        let start = out.len();
        self.case(character, out);
        if self.transform.full_width || self.transform.full_size_kana {
            let converted: String = out[start..]
                .chars()
                .map(|mapped| {
                    let mapped = if self.transform.full_width {
                        width::full_width(mapped, preserved)
                    } else {
                        mapped
                    };
                    if self.transform.full_size_kana {
                        kana::full_size(mapped)
                    } else {
                        mapped
                    }
                })
                .collect();
            out.truncate(start);
            out.push_str(&converted);
        }
        self.advance_word_state(character);
    }

    /// Tells the transformer that something opaque came between two characters.
    ///
    /// An image or an inline block is not a letter, so a word ends at it — and it is not cased, so a
    /// sigma before it is word-final and stays so.
    pub fn interrupt(&mut self) {
        self.word_start = true;
        self.after_cased = false;
        self.pending_sigma = None;
    }

    /// Applies the case half, appending to `out`.
    fn case(&mut self, character: char, out: &mut String) {
        match self.transform.case {
            CaseTransform::None => out.push(character),
            CaseTransform::Upper => case::upper(character, self.language, out),
            CaseTransform::Lower => self.lower(character, out),
            CaseTransform::Capitalize => {
                if self.word_start && case::is_letter_unit(character) {
                    case::title(character, self.language, out);
                } else {
                    out.push(character);
                }
            }
        }
    }

    /// Lower case, with the sigma fix-up the previous character may be owed.
    fn lower(&mut self, character: char, out: &mut String) {
        if case::is_cased(character) {
            // Whatever provisional sigma is outstanding is not word-final after all: a cased letter
            // follows it. Both forms are two bytes, so the correction is in place.
            if let Some(at) = self.pending_sigma.take() {
                out.replace_range(at..at + case::SIGMA_LEN, case::SIGMA_MEDIAL);
            }
        } else if !case::is_case_ignorable(character) {
            self.pending_sigma = None;
        }

        if character == case::SIGMA_CAPITAL && self.after_cased {
            self.pending_sigma = Some(out.len());
            out.push_str(case::SIGMA_FINAL);
            return;
        }
        case::lower(character, self.language, out);
    }

    /// Moves the word and cased state on past `character`.
    fn advance_word_state(&mut self, character: char) {
        if case::is_case_ignorable(character) {
            // Neither a word boundary nor a cased letter: an apostrophe inside a word leaves the
            // word running, which is what makes `don't` capitalise to `Don't`.
            return;
        }
        self.word_start = !case::is_letter_unit(character);
        self.after_cased = case::is_cased(character);
    }
}

#[cfg(test)]
mod tests {
    use zgui_text_style::{CaseTransform, TextTransform};

    use super::Transformer;

    /// Transforms a whole string through one transformer, as the generator does.
    fn run(transform: TextTransform, language: Option<&str>, text: &str) -> String {
        let mut transformer = Transformer::new(transform, language);
        let mut out = String::new();
        for character in text.chars() {
            transformer.push(character, true, &mut out);
        }
        out
    }

    /// A transform with one case keyword and nothing else.
    fn cased(case: CaseTransform) -> TextTransform {
        TextTransform {
            case,
            ..TextTransform::none()
        }
    }

    /// The full mappings: one letter may become several.
    #[test]
    fn upper_case_uses_the_full_mapping() {
        assert_eq!(run(cased(CaseTransform::Upper), None, "straße"), "STRASSE");
        assert_eq!(run(cased(CaseTransform::Upper), None, "ﬁn"), "FIN");
    }

    /// A sigma at the end of a word is written differently from one inside it.
    #[test]
    fn a_word_final_sigma_takes_its_own_form() {
        assert_eq!(run(cased(CaseTransform::Lower), None, "ΟΔΟΣ"), "οδος");
        assert_eq!(run(cased(CaseTransform::Lower), None, "ΟΔΟΣ ΑΒ"), "οδος αβ");
        assert_eq!(run(cased(CaseTransform::Lower), None, "ΣΑ"), "σα");
        // Punctuation is case-ignorable, so a sigma before it is still word-final.
        assert_eq!(run(cased(CaseTransform::Lower), None, "ΟΔΟΣ."), "οδος.");
    }

    /// Turkish keeps the dot on `i` through both directions.
    #[test]
    fn the_turkish_dotted_and_dotless_i_are_kept_apart() {
        assert_eq!(run(cased(CaseTransform::Upper), Some("tr"), "isim"), "İSİM");
        assert_eq!(run(cased(CaseTransform::Lower), Some("tr"), "IsIm"), "ısım");
        assert_eq!(run(cased(CaseTransform::Lower), Some("az"), "I"), "ı");
        // And the default language does not, which is the control.
        assert_eq!(run(cased(CaseTransform::Upper), None, "isim"), "ISIM");
    }

    /// `capitalize` uses title case, which for a digraph is not upper case.
    #[test]
    fn capitalize_title_cases_the_first_letter_of_each_word() {
        assert_eq!(
            run(cased(CaseTransform::Capitalize), None, "ab cd"),
            "Ab Cd"
        );
        assert_eq!(run(cased(CaseTransform::Capitalize), None, "ǆon"), "ǅon");
        assert_eq!(run(cased(CaseTransform::Upper), None, "ǆ"), "Ǆ");
        // An apostrophe does not start a new word, and a digit does not take a case.
        assert_eq!(
            run(cased(CaseTransform::Capitalize), None, "don't"),
            "Don't"
        );
        assert_eq!(
            run(cased(CaseTransform::Capitalize), None, "1st ab"),
            "1st Ab"
        );
    }

    /// The width and kana halves apply on top of the case half, in that order.
    #[test]
    fn the_three_halves_compose() {
        let full_width = TextTransform {
            case: CaseTransform::Upper,
            full_width: true,
            full_size_kana: false,
        };
        assert_eq!(run(full_width, None, "ab"), "ＡＢ");
        assert_eq!(run(full_width, None, " "), "\u{3000}");

        let kana = TextTransform {
            case: CaseTransform::None,
            full_width: false,
            full_size_kana: true,
        };
        assert_eq!(run(kana, None, "ぁっゃ"), "あつや");
    }

    /// A collapsed space is not preserved white space, so `full-width` leaves it alone.
    #[test]
    fn full_width_maps_only_a_preserved_space() {
        let transform = TextTransform {
            case: CaseTransform::None,
            full_width: true,
            full_size_kana: false,
        };
        let mut transformer = Transformer::new(transform, None);
        let mut out = String::new();
        transformer.push(' ', false, &mut out);
        assert_eq!(out, " ");
    }

    /// A transform that changes nothing says so, so the generator can skip it entirely.
    #[test]
    fn the_identity_transform_is_recognisable() {
        assert!(Transformer::new(TextTransform::none(), None).is_identity());
        assert!(!Transformer::new(cased(CaseTransform::Upper), None).is_identity());
    }
}
