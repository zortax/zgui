//! `full-size-kana`: the small kana, replaced by the full-size ones.
//!
//! A fixed list of pairs, given by the CSS Text specification rather than derived from any Unicode
//! property — the small kana are not a range and their full-size counterparts are not at a constant
//! offset. Written out because that is what it is: `ぁ` becomes `あ` and `ゕ` becomes `か`, which no
//! rule connects.
//!
//! Four families: hiragana, katakana, the katakana phonetic extensions, and the half-width forms
//! whose full-size counterparts are also half-width.

/// The full-size kana `character` stands for, or `character` itself.
pub fn full_size(character: char) -> char {
    match character {
        // Hiragana.
        '\u{3041}' => '\u{3042}',
        '\u{3043}' => '\u{3044}',
        '\u{3045}' => '\u{3046}',
        '\u{3047}' => '\u{3048}',
        '\u{3049}' => '\u{304A}',
        '\u{3063}' => '\u{3064}',
        '\u{3083}' => '\u{3084}',
        '\u{3085}' => '\u{3086}',
        '\u{3087}' => '\u{3088}',
        '\u{308E}' => '\u{308F}',
        '\u{3095}' => '\u{304B}',
        '\u{3096}' => '\u{3051}',
        // Katakana.
        '\u{30A1}' => '\u{30A2}',
        '\u{30A3}' => '\u{30A4}',
        '\u{30A5}' => '\u{30A6}',
        '\u{30A7}' => '\u{30A8}',
        '\u{30A9}' => '\u{30AA}',
        '\u{30C3}' => '\u{30C4}',
        '\u{30E3}' => '\u{30E4}',
        '\u{30E5}' => '\u{30E6}',
        '\u{30E7}' => '\u{30E8}',
        '\u{30EE}' => '\u{30EF}',
        '\u{30F5}' => '\u{30AB}',
        '\u{30F6}' => '\u{30B1}',
        // Katakana phonetic extensions, which are small by construction.
        '\u{31F0}' => '\u{30AF}',
        '\u{31F1}' => '\u{30B7}',
        '\u{31F2}' => '\u{30B9}',
        '\u{31F3}' => '\u{30C8}',
        '\u{31F4}' => '\u{30CC}',
        '\u{31F5}' => '\u{30CF}',
        '\u{31F6}' => '\u{30D2}',
        '\u{31F7}' => '\u{30D5}',
        '\u{31F8}' => '\u{30D8}',
        '\u{31F9}' => '\u{30DB}',
        '\u{31FA}' => '\u{30E0}',
        '\u{31FB}' => '\u{30E9}',
        '\u{31FC}' => '\u{30EA}',
        '\u{31FD}' => '\u{30EB}',
        '\u{31FE}' => '\u{30EC}',
        '\u{31FF}' => '\u{30ED}',
        // Half-width katakana: the full-size counterpart is half-width too, because the width
        // belongs to `full-width` and this transform leaves it alone.
        '\u{FF67}' => '\u{FF71}',
        '\u{FF68}' => '\u{FF72}',
        '\u{FF69}' => '\u{FF73}',
        '\u{FF6A}' => '\u{FF74}',
        '\u{FF6B}' => '\u{FF75}',
        '\u{FF6C}' => '\u{FF94}',
        '\u{FF6D}' => '\u{FF95}',
        '\u{FF6E}' => '\u{FF96}',
        '\u{FF6F}' => '\u{FF82}',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::full_size;

    /// Each family maps, and everything else is left as it is.
    #[test]
    fn every_family_of_small_kana_maps_to_its_full_size_form() {
        assert_eq!(full_size('ぁ'), 'あ');
        assert_eq!(full_size('ゖ'), 'け');
        assert_eq!(full_size('ァ'), 'ア');
        assert_eq!(full_size('ヶ'), 'ケ');
        assert_eq!(full_size('\u{31F0}'), 'ク');
        assert_eq!(full_size('\u{FF67}'), '\u{FF71}');
        assert_eq!(full_size('あ'), 'あ', "a full-size kana is already one");
        assert_eq!(full_size('a'), 'a');
    }
}
