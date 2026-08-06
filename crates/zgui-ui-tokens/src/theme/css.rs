//! Reading a block of custom-property declarations back into tokens.
//!
//! This is not a CSS parser and must not become one. A theme's values are opaque text — the style
//! engine is what understands `oklch(…)`, `calc(…)` and `color-mix(…)`, and this crate has always
//! carried them as strings for exactly that reason. What is needed here is only the shape *around*
//! the values: which name each one is for, and where one declaration ends and the next begins.
//!
//! So it splits on semicolons and colons, and hands both sides on untouched. A value containing a
//! semicolon inside a string or a comment would defeat that, and no token value has ever been
//! either; anything genuinely that intricate is a rule in the application's own style sheet, where
//! a real parser sees it.

/// One `--name: value` pair out of a block.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Declaration<'a> {
    /// The custom property's name, including its leading dashes.
    pub(crate) property: &'a str,
    /// Everything after the colon, trimmed.
    pub(crate) value: &'a str,
}

/// Every declaration in `block`, in the order it is written.
///
/// A wrapping `selector { … }` is tolerated so that a theme can be pasted from a style sheet
/// without editing; anything outside the braces that is not a declaration is skipped.
pub(crate) fn declarations(block: &str) -> impl Iterator<Item = Declaration<'_>> {
    let inside = match (block.find('{'), block.rfind('}')) {
        (Some(open), Some(close)) if close > open => &block[open + 1..close],
        _ => block,
    };
    inside
        .split(';')
        .filter_map(|piece| {
            let piece = strip_comments(piece).trim();
            let (property, value) = piece.split_once(':')?;
            Some(Declaration {
                property: property.trim(),
                value: value.trim(),
            })
        })
        .filter(|declaration| {
            declaration.property.starts_with("--") && !declaration.value.is_empty()
        })
}

/// `piece` with any complete `/* … */` run taken out of it.
///
/// Whole comments only. A comment spanning a semicolon has already been cut in two by the split
/// above, and a half-comment is left where it is — it makes the declaration around it
/// unrecognisable, which is what a style sheet does with one as well.
fn strip_comments(piece: &str) -> &str {
    match (piece.find("/*"), piece.rfind("*/")) {
        (Some(open), Some(close)) if close > open => {
            // Whichever side of the comment holds the declaration. A comment before it is much the
            // commoner shape, so that side is preferred when both have text.
            let after = piece[close + 2..].trim();
            if after.is_empty() {
                piece[..open].trim()
            } else {
                after
            }
        }
        _ => piece,
    }
}

#[cfg(test)]
mod tests {
    use super::declarations;

    /// Every declaration in a block, as pairs.
    fn read(block: &str) -> Vec<(&str, &str)> {
        declarations(block)
            .map(|declaration| (declaration.property, declaration.value))
            .collect()
    }

    #[test]
    fn a_bare_block_reads_as_its_declarations() {
        assert_eq!(
            read("--zui-radius-base: 4px; --zui-color-primary: red"),
            [("--zui-radius-base", "4px"), ("--zui-color-primary", "red")]
        );
    }

    #[test]
    fn a_wrapping_rule_is_tolerated_so_a_theme_can_be_pasted_from_a_sheet() {
        assert_eq!(
            read(":root { --zui-radius-base: 0px; }"),
            [("--zui-radius-base", "0px")]
        );
    }

    #[test]
    fn a_value_keeps_its_own_punctuation() {
        // The whole point of carrying values as text: this crate does not know what these mean.
        assert_eq!(
            read(
                "--zui-color-primary: oklch(0.62 0.18 40); --zui-shadow-sm: 0 1px 2px rgb(0 0 0 / 8%)"
            ),
            [
                ("--zui-color-primary", "oklch(0.62 0.18 40)"),
                ("--zui-shadow-sm", "0 1px 2px rgb(0 0 0 / 8%)"),
            ]
        );
    }

    #[test]
    fn anything_that_is_not_a_custom_property_is_skipped() {
        assert_eq!(
            read("color: red; --zui-radius-base: 2px; --broken; --zui-radius-none:"),
            [("--zui-radius-base", "2px")]
        );
    }

    #[test]
    fn a_comment_is_not_part_of_the_declaration_beside_it() {
        assert_eq!(
            read("/* the corners */ --zui-radius-base: 6px;"),
            [("--zui-radius-base", "6px")]
        );
        assert_eq!(
            read("--zui-radius-base: 6px /* the corners */;"),
            [("--zui-radius-base", "6px")]
        );
    }

    #[test]
    fn an_empty_block_reads_as_nothing() {
        assert!(read("").is_empty());
        assert!(read(":root {}").is_empty());
    }
}
