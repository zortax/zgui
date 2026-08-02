//! Where one caret step lands, when nothing has been shaped yet.
//!
//! The authority on this is the shaper: a cluster is what can be selected, and which characters
//! form one is a property of the font's own mapping — a ligature is one cluster of several
//! characters, an Indic conjunct is one cluster of many, and a caret may not be placed inside
//! either. [`crate::hit::LineMap`] answers from exactly those clusters and is what an editor over
//! laid-out text uses.
//!
//! This is the answer for the text that has *not* been laid out — a field being built, a paste
//! being sanitised, a headless test — and it is deliberately a documented approximation rather
//! than a second Unicode implementation. It never splits a character, and it keeps these together
//! with the character before them:
//!
//! * combining marks in the ranges the great majority of text uses: U+0300–U+036F, U+0483–U+0489,
//!   U+0591–U+05BD, U+0610–U+061A, U+064B–U+065F, U+0670, U+06D6–U+06DC, U+0E31–U+0E3A,
//!   U+0E47–U+0E4E, U+1AB0–U+1AFF, U+1DC0–U+1DFF, U+20D0–U+20F0 and U+FE20–U+FE2F;
//! * variation selectors, U+FE00–U+FE0F and U+E0100–U+E01EF;
//! * a zero-width joiner and whatever follows it, which is what holds an emoji sequence together;
//! * a pair of regional indicators, which is what a flag is.
//!
//! What it does not cover is the rest of UAX#29 — Hangul syllable composition, prepended
//! concatenation marks, and every mark outside the ranges above. A caret in such text steps by one
//! character instead of one grapheme, which is visible and harmless, where splitting a character
//! would corrupt the text.

/// The offset one grapheme after `offset`, or the end of the text.
pub fn next(text: &str, offset: usize) -> usize {
    let offset = clamp(text, offset);
    let mut cursor = offset;
    let mut joined = false;
    let mut regional = 0usize;
    while cursor < text.len() {
        let character = char_at(text, cursor);
        let width = character.len_utf8();
        let extends = cursor > offset && !joined && is_extending(character);
        let pair = cursor > offset && regional == 1 && is_regional(character);
        if cursor > offset && !extends && !joined && !pair {
            break;
        }
        if is_regional(character) {
            regional += 1;
        }
        joined = character == ZWJ;
        cursor += width;
    }
    cursor
}

/// The offset one grapheme before `offset`, or the start of the text.
pub fn previous(text: &str, offset: usize) -> usize {
    let offset = clamp(text, offset);
    if offset == 0 {
        return 0;
    }
    // Backwards is forwards from far enough back, which keeps the rules in [`next`] alone. Far
    // enough is [`RUN_UP`] characters: a step from a position inside a cluster still ends at that
    // cluster's end, so after the first step every position visited is a real boundary, and a run
    // up longer than any grapheme guarantees at least one step before the target.
    let mut boundary = offset;
    for _ in 0..RUN_UP {
        if boundary == 0 {
            break;
        }
        boundary = previous_char(text, boundary);
    }
    loop {
        let step = next(text, boundary);
        if step >= offset {
            return boundary;
        }
        boundary = step;
    }
}

/// How many characters back a backwards step starts from.
///
/// Longer than any grapheme this module recognises — the longest is an emoji sequence of a handful
/// of joined characters — and short enough that stepping the caret left costs nothing measurable.
const RUN_UP: usize = 64;

/// A zero-width joiner, which binds what follows it to what precedes it.
const ZWJ: char = '\u{200d}';

/// The character starting at `offset`.
fn char_at(text: &str, offset: usize) -> char {
    text[offset..].chars().next().unwrap_or('\0')
}

/// The offset of the character before `offset`.
fn previous_char(text: &str, offset: usize) -> usize {
    let mut candidate = offset - 1;
    while !text.is_char_boundary(candidate) {
        candidate -= 1;
    }
    candidate
}

/// The nearest character boundary at or before `offset`.
fn clamp(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Whether a character attaches to the one before it.
fn is_extending(character: char) -> bool {
    matches!(character as u32,
        0x0300..=0x036f
        | 0x0483..=0x0489
        | 0x0591..=0x05bd
        | 0x0610..=0x061a
        | 0x064b..=0x065f
        | 0x0670
        | 0x06d6..=0x06dc
        | 0x0e31..=0x0e3a
        | 0x0e47..=0x0e4e
        | 0x1ab0..=0x1aff
        | 0x1dc0..=0x1dff
        | 0x200d
        | 0x20d0..=0x20f0
        | 0xfe00..=0xfe0f
        | 0xfe20..=0xfe2f
        | 0xe0100..=0xe01ef)
}

/// Whether a character is half of a flag.
fn is_regional(character: char) -> bool {
    matches!(character as u32, 0x1f1e6..=0x1f1ff)
}

#[cfg(test)]
mod tests {
    use super::{next, previous};

    #[test]
    fn a_plain_character_is_one_step_in_both_directions() {
        assert_eq!(next("abc", 1), 2);
        assert_eq!(previous("abc", 2), 1);
    }

    #[test]
    fn a_multibyte_character_is_never_split() {
        let text = "aé b";
        assert_eq!(next(text, 1), 3, "é is two bytes and one step");
        assert_eq!(previous(text, 3), 1);
    }

    #[test]
    fn a_combining_mark_travels_with_the_letter_it_sits_on() {
        // "e" then U+0301 COMBINING ACUTE ACCENT: one grapheme of three bytes.
        let text = "e\u{301}x";
        assert_eq!(next(text, 0), 3);
        assert_eq!(previous(text, 3), 0);
    }

    #[test]
    fn an_emoji_joined_by_a_zero_width_joiner_is_one_grapheme() {
        let text = "\u{1f469}\u{200d}\u{1f4bb}!";
        assert_eq!(next(text, 0), text.len() - 1);
        assert_eq!(previous(text, text.len() - 1), 0);
    }

    #[test]
    fn a_flag_is_two_regional_indicators_and_one_step() {
        let text = "\u{1f1e6}\u{1f1e8}\u{1f1e6}\u{1f1e8}";
        assert_eq!(next(text, 0), 8, "one flag");
        assert_eq!(next(text, 8), 16, "then the next");
        assert_eq!(previous(text, 16), 8);
    }

    #[test]
    fn the_ends_of_the_text_are_fixed_points() {
        assert_eq!(next("abc", 3), 3);
        assert_eq!(previous("abc", 0), 0);
        assert_eq!(next("", 0), 0);
    }

    #[test]
    fn an_offset_inside_a_character_is_clamped_rather_than_panicking() {
        let text = "é";
        assert_eq!(next(text, 1), 2);
        assert_eq!(previous(text, 1), 0);
    }
}
