//! Checking CSS where it is written.
//!
//! The check is structural: strings and comments are terminated, blocks are balanced, a rule has
//! a selector, a declaration has a value, and nothing in the text is a comment CSS does not have.
//! That is the class of mistake a typed sheet makes, and catching it here means it is reported
//! against the source rather than warned about once, at run time, when the sheet is loaded.
//!
//! # Why `//` is refused
//!
//! CSS has one comment, `/* … */`. A `//` inside the text is not one, and the damage it does is
//! silent and unbounded: a declaration parser recovering from it consumes everything up to the
//! next semicolon, so the line after the supposed comment is swallowed with it. A sheet written
//! that way installs, matches, and quietly lacks whichever declaration followed the words
//! explaining it.

use proc_macro2::Span;

/// Where in the text a problem was found.
struct Position {
    /// Line, counting from one.
    line: usize,
    /// Column, counting from one.
    column: usize,
}

/// Checks one block of CSS.
pub(crate) fn validate(css: &str, span: Span) -> syn::Result<()> {
    let bytes: Vec<char> = css.chars().collect();
    let mut index = 0;
    let mut depth: Vec<usize> = Vec::new();
    let mut segment_start = 0usize;
    let mut brace_depth = 0usize;

    while index < bytes.len() {
        let character = bytes[index];
        match character {
            '/' if bytes.get(index + 1) == Some(&'*') => {
                let start = index;
                index += 2;
                loop {
                    if index + 1 >= bytes.len() {
                        return Err(error(css, start, span, "this comment is never closed"));
                    }
                    if bytes[index] == '*' && bytes[index + 1] == '/' {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
                continue;
            }
            // A scheme's `//` is part of a URL rather than a comment, and `https://…` is the one
            // place two slashes belong in a sheet.
            '/' if bytes.get(index + 1) == Some(&'/')
                && index.checked_sub(1).map(|before| bytes[before]) != Some(':') =>
            {
                return Err(error(
                    css,
                    index,
                    span,
                    "`//` is not a comment in CSS\n\n\
                     help: write `/* … */`, or put the remark between the rules as a Rust comment\n\
                     note: the parser recovering from it swallows the declaration that follows",
                ));
            }
            '"' | '\'' => {
                let start = index;
                index += 1;
                loop {
                    if index >= bytes.len() || bytes[index] == '\n' {
                        return Err(error(css, start, span, "this string is never closed"));
                    }
                    if bytes[index] == '\\' {
                        index += 2;
                        continue;
                    }
                    if bytes[index] == character {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                continue;
            }
            '(' | '[' => depth.push(index),
            ')' | ']' if depth.pop().is_none() => {
                return Err(error(
                    css,
                    index,
                    span,
                    &format!("`{character}` closes something that was never opened"),
                ));
            }
            '{' => {
                let prelude = css_slice(&bytes, segment_start, index);
                if prelude.trim().is_empty() {
                    return Err(error(
                        css,
                        index,
                        span,
                        "this block has no selector\n\n\
                         help: a rule is written `selector { … }`",
                    ));
                }
                brace_depth += 1;
                segment_start = index + 1;
            }
            '}' => {
                if brace_depth == 0 {
                    return Err(error(
                        css,
                        index,
                        span,
                        "`}` closes a block that was never opened",
                    ));
                }
                check_declaration(&bytes, segment_start, index, css, span)?;
                brace_depth -= 1;
                segment_start = index + 1;
            }
            ';' => {
                if brace_depth > 0 {
                    check_declaration(&bytes, segment_start, index, css, span)?;
                }
                segment_start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    if let Some(open) = depth.first() {
        return Err(error(css, *open, span, "this is never closed"));
    }
    if brace_depth > 0 {
        let opened = bytes.iter().rposition(|character| *character == '{');
        return Err(error(
            css,
            opened.unwrap_or(0),
            span,
            "this block is never closed",
        ));
    }
    Ok(())
}

/// Rejects a declaration with no value, which is the typo a stylesheet actually makes.
fn check_declaration(
    text: &[char],
    start: usize,
    end: usize,
    css: &str,
    span: Span,
) -> syn::Result<()> {
    let segment = css_slice(text, start, end);
    let trimmed = segment.trim();
    if trimmed.is_empty() || trimmed.starts_with('@') || trimmed.contains(':') {
        return Ok(());
    }
    Err(error(
        css,
        start,
        span,
        &format!(
            "`{trimmed}` is not a declaration\n\n\
             help: a declaration is written `property: value`"
        ),
    ))
}

/// The text between two character offsets.
fn css_slice(text: &[char], start: usize, end: usize) -> String {
    text[start.min(text.len())..end.min(text.len())]
        .iter()
        .collect()
}

/// Builds the diagnostic, naming where in the block the problem is.
fn error(css: &str, offset: usize, span: Span, message: &str) -> syn::Error {
    let position = position_of(css, offset);
    syn::Error::new(
        span,
        format!(
            "{message}\n\nnote: in the CSS at line {}, column {}",
            position.line, position.column
        ),
    )
}

/// The line and column of a character offset.
fn position_of(css: &str, offset: usize) -> Position {
    let mut line = 1;
    let mut column = 1;
    for (index, character) in css.chars().enumerate() {
        if index >= offset {
            break;
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    Position { line, column }
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;

    use super::validate;

    fn check(css: &str) -> Result<(), String> {
        validate(css, Span::call_site()).map_err(|error| error.to_string())
    }

    #[test]
    fn a_well_formed_sheet_passes() {
        check(".a { color: red; } @media (min-width: 40rem) { .a { color: blue } }")
            .expect("this sheet is well formed");
        check("/* a comment { */ .a::before { content: \"}\"; }")
            .expect("delimiters inside strings and comments are not delimiters");
    }

    #[test]
    fn an_unclosed_block_is_reported_with_its_position() {
        let error = check(".a { color: red;").expect_err("the block is never closed");
        assert!(error.contains("never closed"), "{error}");
        assert!(error.contains("line 1"), "{error}");
    }

    #[test]
    fn a_declaration_with_no_value_is_reported() {
        let error = check(".a { color red; }").expect_err("`color red` is not a declaration");
        assert!(error.contains("not a declaration"), "{error}");
    }

    #[test]
    fn a_rule_with_no_selector_is_reported() {
        let error = check("  { color: red; }").expect_err("the rule has no selector");
        assert!(error.contains("no selector"), "{error}");
    }

    #[test]
    fn a_line_comment_is_reported_because_css_has_none() {
        // The shape that costs a declaration: the words explaining it run to the semicolon that
        // ends it, so a sheet accepting this would install without the property it is about.
        let error = check(".surface {\n  // where it sits\n  transform: translate(-50%, -50%);\n}")
            .expect_err("`//` is not a comment");
        assert!(error.contains("not a comment in CSS"), "{error}");
        assert!(error.contains("line 2"), "{error}");
    }

    #[test]
    fn a_scheme_and_a_string_may_hold_two_slashes() {
        check("@import url(https://example.test/a.css);")
            .expect("a scheme's slashes are part of a URL");
        check(".a::before { content: \"//\"; }").expect("a string is not read as CSS");
        check("/* // */ .a { color: red }").expect("a comment is not read as CSS");
    }

    #[test]
    fn an_unterminated_string_is_reported() {
        let error = check(".a::before { content: \"oops; }").expect_err("the string is open");
        assert!(error.contains("string is never closed"), "{error}");
    }
}
