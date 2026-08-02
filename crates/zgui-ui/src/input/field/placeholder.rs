//! The line a field shows while there is nothing in it.
//!
//! # Why it is not an element
//!
//! A field's element holds exactly the text nodes the editing model writes and nothing else, and
//! that is what makes `:empty` mean *this field has no text*. An element child — a box holding the
//! placeholder — would make every field non-empty for ever, and the placeholder would need a second
//! answer to "is there any text", kept by the component, from a copy of the text the component must
//! not have.
//!
//! So the placeholder is generated content on `::before`, its text carried in a custom property so
//! that one rule serves every instance, and it is taken out of flow: an in-flow box before the text
//! would push the insertion point along behind it, and the caret of an empty field belongs at the
//! start of the field rather than at the end of its placeholder.

use zgui::prelude::Attrs;
use zgui::view::CustomPropertyName;

/// The custom property a field's placeholder text is carried in, without its leading dashes.
pub(crate) const PLACEHOLDER: &str = "zui-field-placeholder";

/// Adds the placeholder text to what a field's element carries, when it has one.
pub(crate) fn declared(attrs: Attrs, placeholder: Option<&str>) -> Attrs {
    let Some(text) = placeholder else {
        return attrs;
    };
    let quoted = quote(text);
    attrs.custom_property(CustomPropertyName::new(PLACEHOLDER), move || {
        Some(quoted.clone())
    })
}

/// `text` as a CSS string token, quotes and backslashes escaped.
///
/// A placeholder is whatever an application wrote, so it can hold a quote — and a quote written
/// straight into a declaration ends the string early and leaves the rest as garbage the parser
/// drops, which is a field whose placeholder silently disappears.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' | '\\' => {
                out.push('\\');
                out.push(character);
            }
            // A literal newline ends a CSS string. The escape needs the trailing space, or the
            // character after it is read as another hexadecimal digit of the escape.
            '\n' => out.push_str("\\A "),
            _ => out.push(character),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::quote;

    #[test]
    fn ordinary_text_is_wrapped_and_left_alone() {
        assert_eq!(quote("you@example.com"), "\"you@example.com\"");
    }

    #[test]
    fn a_quote_in_the_text_does_not_end_the_string_early() {
        assert_eq!(quote("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(quote("one\ntwo"), "\"one\\A two\"");
    }
}
