//! Whether a document asked for text this crate does not draw.
//!
//! This crate reads documents with the parser's own text support switched off, because turning it
//! on would link a second font stack beside the one this framework already shapes every paragraph
//! with — two font databases, two shapers, two answers to "what does this string look like". So a
//! `<text>` element never becomes a node at all: it is gone before there is a tree to walk.
//!
//! Gone silently is the problem. A caller handed an empty drawing has no way to tell a document
//! this cannot draw from a document that draws nothing, so the elements are counted here, off the
//! source, and reported alongside everything else the model cannot carry.

/// How many text elements the source asks for.
///
/// Counted by looking for the element in the markup, which is the only place left to look. Markup
/// inside a comment does not count; an element name that merely starts with the same letters —
/// `<textPath>` is one, `<tspan>` is not — is matched by what follows it rather than by a prefix.
pub(crate) fn count(source: &str) -> u32 {
    let mut found = 0;
    let mut rest = source;
    while let Some(open) = rest.find('<') {
        rest = &rest[open..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = match after.find("-->") {
                Some(close) => &after[close + 3..],
                None => return found,
            };
            continue;
        }
        let body = &rest[1..];
        if starts_element(body, "text") || starts_element(body, "tspan") {
            found += 1;
        }
        rest = body;
    }
    found
}

/// Whether `body` begins the named element rather than one whose name starts the same way.
fn starts_element(body: &str, name: &str) -> bool {
    let Some(after) = body.strip_prefix(name) else {
        return false;
    };
    match after.chars().next() {
        None => false,
        Some(character) => character.is_whitespace() || character == '>' || character == '/',
    }
}

#[cfg(test)]
mod tests {
    use super::count;

    #[test]
    fn a_text_element_is_counted_however_it_is_written() {
        assert_eq!(count("<text x='0'>hello</text>"), 1);
        assert_eq!(count("<text/>"), 1);
        assert_eq!(count("<text>a<tspan>b</tspan></text>"), 2);
    }

    #[test]
    fn an_element_whose_name_merely_starts_the_same_way_is_not_text() {
        assert_eq!(count("<textPath href='#p'>hello</textPath>"), 0);
        assert_eq!(count("<texture/>"), 0);
    }

    #[test]
    fn markup_inside_a_comment_asks_for_nothing() {
        assert_eq!(count("<!-- <text>not really</text> --><rect/>"), 0);
        assert_eq!(count("<!-- unterminated <text>"), 0);
    }

    #[test]
    fn a_document_with_no_text_counts_none() {
        assert_eq!(count("<svg><rect/><g><path/></g></svg>"), 0);
        assert_eq!(count(""), 0);
    }
}
