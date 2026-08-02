//! Finding a tag in a view's own text, without parsing anything.
//!
//! The scan is deliberately narrow. It walks to each `view!`, takes the balanced group after it,
//! and looks inside that group only — so a generic argument list in ordinary code, a `<kbd>` in a
//! documentation table and an SVG document in a string are all invisible to it. Inside the group
//! the two markers it looks for cannot be written any other way: no operator ends in `/>`, and
//! `</` closes nothing the new grammar opens.

/// The markers a view may not contain.
const TAGS: [&str; 2] = ["</", "/>"];

/// Every tag written inside a view in `text`, as a line number and the marker found.
pub(super) fn tags(text: &str) -> Vec<(usize, &'static str)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = text[at..].find("view!") {
        let start = at + offset;
        at = start + "view!".len();
        let Some(body) = group(text, at) else {
            continue;
        };
        for (inner, marker) in markers(&text[body.clone()]) {
            found.push((line(text, body.start + inner), marker));
        }
        at = body.end;
    }
    let _ = bytes;
    found
}

/// The balanced group beginning at the first delimiter after `at`, if the next thing is one.
fn group(text: &str, at: usize) -> Option<std::ops::Range<usize>> {
    let rest = &text[at..];
    let lead = rest.len() - rest.trim_start().len();
    let open = rest.as_bytes().get(lead)?;
    let close = match open {
        b'{' => b'}',
        b'(' => b')',
        b'[' => b']',
        _ => return None,
    };
    let mut depth = 0usize;
    let bytes = rest.as_bytes();
    for (index, byte) in bytes.iter().enumerate().skip(lead) {
        if byte == open {
            depth += 1;
        } else if *byte == close {
            depth -= 1;
            if depth == 0 {
                return Some(at + lead + 1..at + index);
            }
        }
    }
    None
}

/// Every marker in one view's text, by offset, ignoring string literals and comments.
///
/// A string child may say anything, including `</p>`, and a commented-out line is not code. Both
/// would otherwise be reported as views nobody has migrated.
fn markers(body: &str) -> Vec<(usize, &'static str)> {
    let bytes = body.as_bytes();
    let mut found = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += body[index..].find('\n').unwrap_or(body.len() - index);
            }
            _ => {
                if let Some(marker) = TAGS
                    .iter()
                    .find(|marker| body[index..].starts_with(*marker))
                {
                    found.push((index, *marker));
                }
                index += 1;
            }
        }
    }
    found
}

/// Which line of `text` an offset is on, counting from one.
fn line(text: &str, offset: usize) -> usize {
    text[..offset].lines().count().max(1)
}

#[cfg(test)]
mod tests {
    use super::tags;

    #[test]
    fn a_tag_inside_a_view_is_found_with_its_line() {
        let text = "fn a() {\n    view! {\n        <row/>\n    }\n}\n";
        assert_eq!(tags(text), vec![(3, "/>")]);
    }

    #[test]
    fn a_call_and_a_block_is_not_a_tag() {
        let text = "view! {\n    row(class = \"a\") {\n        text {\"hi\"}\n    }\n}\n";
        assert!(tags(text).is_empty());
    }

    #[test]
    fn nothing_outside_a_view_is_read() {
        let text = "/// | <kbd>Enter</kbd> | activates |\nconst SVG: &str = \"<svg/>\";\n\
                    fn a() -> Vec<u8> { Vec::<u8>::new() }\n";
        assert!(tags(text).is_empty());
    }

    #[test]
    fn a_string_child_may_say_anything() {
        assert!(tags("view! { text {\"</p>\"} }").is_empty());
        assert!(tags("view! {\n    // <row/>\n    row()\n}").is_empty());
    }

    #[test]
    fn the_macro_named_without_a_body_is_not_a_view() {
        assert!(tags("view!     := node*\n<row/>\n").is_empty());
    }

    #[test]
    fn two_views_are_both_read() {
        let text = "view! { <a/> }\nview! { b() }\nview! { <c/> }\n";
        assert_eq!(tags(text), vec![(1, "/>"), (3, "/>")]);
    }
}
