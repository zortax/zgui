//! Which script a character belongs to, in the form a fallback lookup wants it.

use fontique::Script;
use swash::text::Codepoint;

/// The script of one character, as an ISO 15924 code.
///
/// Fallback is per character rather than per run: a Latin sentence with one Arabic word in it
/// needs two faces, and a lookup keyed on the run as a whole cannot express that.
///
/// # The spelling, which is the only subtle part
///
/// A script has two four-letter spellings — the ISO 15924 code (`Arab`) and the OpenType tag
/// (`arab`) — and a fallback list is keyed on the first while character properties are reported in
/// the second. They differ only in the case of the first letter for every script that has a single
/// OpenType tag, so that is the conversion; a script whose OpenType tag is one of the versioned
/// forms (`dev2` beside `deva`) resolves to a code no fallback list holds, and falls through to no
/// fallback rather than to the wrong one.
///
/// ```
/// use zgui_text_parley::{Script, script_of};
///
/// assert_eq!(script_of('م'), Script::from_bytes(*b"Arab"));
/// assert_eq!(script_of('a'), Script::from_bytes(*b"Latn"));
/// ```
pub fn script_of(character: char) -> Script {
    let tag = character.script().to_opentype().to_be_bytes();
    Script::from_bytes([tag[0].to_ascii_uppercase(), tag[1], tag[2], tag[3]])
}
