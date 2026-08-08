//! `full-width`: the narrow and half-width forms, replaced by their wide equivalents.
//!
//! This is Unicode's own `<wide>` compatibility mapping, read backwards: a character has a
//! full-width form exactly when some character in the Halfwidth and Fullwidth Forms block
//! decomposes to it. The mapping is one character to one character throughout: a half-width
//! katakana followed by a half-width voiced sound mark maps to two full-width characters, leaving
//! them uncomposed. That is what the compatibility mapping says, and it is what keeps this
//! transform from having to look at more than one character at a time.
//!
//! The space is the one exception the specification makes: `U+0020` becomes an ideographic space
//! only inside white space the style preserves, because a collapsed run of white space has already
//! become a single space standing for the run rather than a character the document wrote.

/// The full-width form of `character`, or `character` itself.
///
/// `preserved` says whether a space here is white space that survives as a space.
pub fn full_width(character: char, preserved: bool) -> char {
    if character == ' ' {
        return if preserved { '\u{3000}' } else { ' ' };
    }
    let code = character as u32;
    let wide = match code {
        // The printable ASCII range, which is a constant offset away from its full-width forms.
        0x0021..=0x007E => code + 0xFEE0,
        // The currency and other signs whose full-width forms are at the end of the block.
        0x00A2 => 0xFFE0,
        0x00A3 => 0xFFE1,
        0x00AC => 0xFFE2,
        0x00AF => 0xFFE3,
        0x00A6 => 0xFFE4,
        0x00A5 => 0xFFE5,
        0x20A9 => 0xFFE6,
        // Half-width CJK punctuation.
        0xFF61 => 0x3002,
        0xFF62 => 0x300C,
        0xFF63 => 0x300D,
        0xFF64 => 0x3001,
        0xFF65 => 0x30FB,
        // Half-width katakana: the small ones, then the prolonged sound mark, then the rest.
        0xFF66 => 0x30F2,
        0xFF67 => 0x30A1,
        0xFF68 => 0x30A3,
        0xFF69 => 0x30A5,
        0xFF6A => 0x30A7,
        0xFF6B => 0x30A9,
        0xFF6C => 0x30E3,
        0xFF6D => 0x30E5,
        0xFF6E => 0x30E7,
        0xFF6F => 0x30C3,
        0xFF70 => 0x30FC,
        0xFF71..=0xFF9D => KATAKANA[(code - 0xFF71) as usize],
        0xFF9E => 0x309B,
        0xFF9F => 0x309C,
        // Half-width Hangul: the filler, the consonants, and the four runs of vowels.
        0xFFA0 => 0x3164,
        0xFFA1..=0xFFBE => code - 0xFFA1 + 0x3131,
        0xFFC2..=0xFFC7 => code - 0xFFC2 + 0x314F,
        0xFFCA..=0xFFCF => code - 0xFFCA + 0x3155,
        0xFFD2..=0xFFD7 => code - 0xFFD2 + 0x315B,
        0xFFDA..=0xFFDC => code - 0xFFDA + 0x3161,
        // Half-width forms of the box-drawing and geometric characters.
        0xFFE8 => 0x2502,
        0xFFE9 => 0x2190,
        0xFFEA => 0x2191,
        0xFFEB => 0x2192,
        0xFFEC => 0x2193,
        0xFFED => 0x25A0,
        0xFFEE => 0x25CB,
        _ => return character,
    };
    char::from_u32(wide).unwrap_or(character)
}

/// The full-width katakana `U+FF71..=U+FF9D` map to, in order.
///
/// Written out because the two blocks are not parallel: the half-width block has one code point per
/// syllable while the full-width one interleaves the voiced forms, so no offset connects them.
const KATAKANA: [u32; 45] = [
    0x30A2, 0x30A4, 0x30A6, 0x30A8, 0x30AA, // a i u e o
    0x30AB, 0x30AD, 0x30AF, 0x30B1, 0x30B3, // ka ki ku ke ko
    0x30B5, 0x30B7, 0x30B9, 0x30BB, 0x30BD, // sa shi su se so
    0x30BF, 0x30C1, 0x30C4, 0x30C6, 0x30C8, // ta chi tsu te to
    0x30CA, 0x30CB, 0x30CC, 0x30CD, 0x30CE, // na ni nu ne no
    0x30CF, 0x30D2, 0x30D5, 0x30D8, 0x30DB, // ha hi fu he ho
    0x30DE, 0x30DF, 0x30E0, 0x30E1, 0x30E2, // ma mi mu me mo
    0x30E4, 0x30E6, 0x30E8, // ya yu yo
    0x30E9, 0x30EA, 0x30EB, 0x30EC, 0x30ED, // ra ri ru re ro
    0x30EF, 0x30F3, // wa n
];

#[cfg(test)]
mod tests {
    use super::full_width;

    /// The ASCII range, the signs and the half-width blocks all map.
    #[test]
    fn every_narrow_form_maps_to_its_wide_one() {
        assert_eq!(full_width('a', true), 'ａ');
        assert_eq!(full_width('!', true), '！');
        assert_eq!(full_width('~', true), '～');
        assert_eq!(full_width('¥', true), '￥');
        assert_eq!(full_width('\u{FF71}', true), 'ア');
        assert_eq!(full_width('\u{FF9D}', true), 'ン');
        assert_eq!(full_width('\u{FF61}', true), '。');
        assert_eq!(full_width('\u{FFA1}', true), '\u{3131}');
    }

    /// A space maps only where it is white space the style keeps.
    #[test]
    fn a_space_maps_only_when_it_is_preserved() {
        assert_eq!(full_width(' ', true), '\u{3000}');
        assert_eq!(full_width(' ', false), ' ');
    }

    /// Anything with no full-width form is left exactly as it was.
    #[test]
    fn a_character_with_no_wide_form_is_left_alone() {
        for character in ['あ', 'Ａ', '\u{3000}', '\u{0301}', '€'] {
            assert_eq!(full_width(character, true), character, "{character:?}");
        }
    }
}
