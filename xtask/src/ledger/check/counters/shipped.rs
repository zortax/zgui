//! Which text in a member counts as the code it ships.
//!
//! Two things are excluded, for one reason. A counter is *produced* by a stage doing work, so a
//! counter incremented only from a test — an integration test under `tests/`, an item behind
//! `#[cfg(test)]` — is not produced at all: the budget written against it would be measuring the
//! test that moves it. The same cut applies to reading the counter set out of the tree, because a
//! set declared inside a test module is a fixture rather than the framework's counters.
//!
//! The cut removes each `#[cfg(test)]` item and keeps the text around it. A test-only helper inside
//! an `impl` therefore hides nothing below it, and a test module can sit anywhere in its file.

use std::borrow::Cow;

use crate::ledger::tree::sources::SourceFile;

/// The path fragment that separates a member's shipped code from its test targets.
const SHIPPED: &str = "/src/";

/// The attribute that marks an item compiled only for tests.
const TEST_ONLY: &[u8] = b"#[cfg(test)]";

/// The words that begin an item whose end is its `;` or its braced body.
const ITEM_KEYWORDS: &[&[u8]] = &[
    b"async",
    b"enum",
    b"extern",
    b"fn",
    b"impl",
    b"macro_rules",
    b"mod",
    b"struct",
    b"trait",
    b"union",
    b"unsafe",
];

/// The words that begin an item or a statement whose end is its `;` only.
///
/// A braced group in such an item is part of a path or a value, so it ends nothing.
const STATEMENT_KEYWORDS: &[&[u8]] = &[b"const", b"let", b"static", b"type", b"use"];

/// The words after `const` that make it a qualifier of a function.
const CONST_QUALIFIED: &[&[u8]] = &[b"async", b"extern", b"fn", b"unsafe"];

/// How the item after a `#[cfg(test)]` ends.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// After its `;` or its braced body.
    Item,
    /// After its `;`.
    Statement,
    /// After its `;` or its `,`: a field, an enum variant, a match arm or an expression.
    Listed,
}

/// The part of `file` that is compiled into the crate, or `None` when the file is a test target.
pub(crate) fn code_of(file: &SourceFile) -> Option<Cow<'_, str>> {
    if !file.rel_path.contains(SHIPPED) {
        return None;
    }
    Some(without_test_items(&file.text))
}

/// `text` with every item that follows a `#[cfg(test)]` removed, attribute included.
///
/// The attribute is only found in code: a comment or a string that mentions it removes nothing.
fn without_test_items(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut kept = String::new();
    let mut from = 0;
    let mut at = 0;
    let mut cut = false;
    while at < bytes.len() {
        if bytes[at..].starts_with(TEST_ONLY) {
            kept.push_str(&text[from..at]);
            at = item_end(bytes, at + TEST_ONLY.len());
            from = at;
            cut = true;
        } else {
            at = after_token(bytes, at);
        }
    }
    if !cut {
        return Cow::Borrowed(text);
    }
    kept.push_str(&text[from..]);
    Cow::Owned(kept)
}

/// Where the item that begins at or after `at` ends.
///
/// The item ends as its [`Shape`] says, and only at a delimiter outside parentheses and brackets.
/// A closing delimiter that has no opener in the item ends the enclosing block, so the item stops
/// before it.
fn item_end(bytes: &[u8], at: usize) -> usize {
    let start = after_trivia(bytes, at);
    let shape = shape_at(bytes, start);
    let listed = shape == Shape::Listed;
    let mut depth = 0usize;
    let mut angles = 0usize;
    let mut body = false;
    let mut at = start;
    while at < bytes.len() {
        match bytes[at] {
            b'{' | b'(' | b'[' => {
                body |= depth == 0 && bytes[at] == b'{' && shape == Shape::Item;
                depth += 1;
            }
            b'}' | b')' | b']' => {
                if depth == 0 {
                    return at;
                }
                depth -= 1;
                if depth == 0 && body {
                    return at + 1;
                }
            }
            b';' if depth == 0 => return at + 1,
            b',' if depth == 0 && listed && angles == 0 => return at + 1,
            // Any other `<` is a comparison or a shift, which opens nothing.
            b'<' if listed && opens_generics(bytes, at) => angles += 1,
            // `->` and `=>` are arrows, which close no generic argument list.
            b'>' if listed && !matches!(bytes[at - 1], b'-' | b'=') => {
                angles = angles.saturating_sub(1);
            }
            _ => {}
        }
        at = after_token(bytes, at);
    }
    bytes.len()
}

/// The first position at or after `at` that is outside whitespace, comments and outer attributes.
fn after_trivia(bytes: &[u8], mut at: usize) -> usize {
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let rest = &bytes[at..];
        if rest.starts_with(b"//") || rest.starts_with(b"/*") {
            at = after_token(bytes, at);
        } else if rest.starts_with(b"#[") {
            at = after_group(bytes, at + 1);
        } else {
            return at;
        }
    }
}

/// The position after the delimited group that opens at `at`.
fn after_group(bytes: &[u8], mut at: usize) -> usize {
    let mut depth = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return at + 1;
                }
            }
            _ => {}
        }
        at = after_token(bytes, at);
    }
    bytes.len()
}

/// The shape of the item at `at`, from its first word after any visibility.
fn shape_at(bytes: &[u8], mut at: usize) -> Shape {
    if word_at(bytes, at) == b"pub" {
        at = after_trivia(bytes, at + 3);
        if bytes.get(at) == Some(&b'(') {
            at = after_group(bytes, at);
        }
        at = after_trivia(bytes, at);
    }
    let word = word_at(bytes, at);
    if word == b"const" {
        let next = word_at(bytes, after_trivia(bytes, at + word.len()));
        if CONST_QUALIFIED.contains(&next) {
            return Shape::Item;
        }
    }
    if STATEMENT_KEYWORDS.contains(&word) {
        Shape::Statement
    } else if ITEM_KEYWORDS.contains(&word) {
        Shape::Item
    } else {
        Shape::Listed
    }
}

/// Whether the `<` at `at` opens generic arguments: it follows an identifier or a `::` directly.
fn opens_generics(bytes: &[u8], at: usize) -> bool {
    at > 0 && (is_identifier(bytes[at - 1]) || bytes[at - 1] == b':')
}

/// The identifier that begins at `at`, empty if none does.
fn word_at(bytes: &[u8], at: usize) -> &[u8] {
    let length = bytes[at..]
        .iter()
        .take_while(|&&byte| is_identifier(byte))
        .count();
    &bytes[at..at + length]
}

/// Whether `byte` can be part of an identifier.
fn is_identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The position after the token at `at`, which steps over a whole comment or literal at once.
///
/// Every other byte is a token of its own, which is enough for the delimiters the cut counts.
fn after_token(bytes: &[u8], at: usize) -> usize {
    let rest = &bytes[at..];
    if rest.starts_with(b"//") {
        return rest
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(bytes.len(), |line_end| at + line_end);
    }
    if rest.starts_with(b"/*") {
        return after_block_comment(bytes, at);
    }
    match rest[0] {
        b'"' => after_string(bytes, at + 1),
        b'\'' => after_quote(bytes, at),
        b'r' if starts_raw_string(bytes, at) => after_raw_string(bytes, at + 1),
        _ => at + 1,
    }
}

/// The position after the block comment at `at`, which can nest.
fn after_block_comment(bytes: &[u8], mut at: usize) -> usize {
    let mut depth = 0usize;
    while at < bytes.len() {
        if bytes[at..].starts_with(b"/*") {
            depth += 1;
            at += 2;
        } else if bytes[at..].starts_with(b"*/") {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return at;
            }
        } else {
            at += 1;
        }
    }
    bytes.len()
}

/// The position after the closing quote of a string whose body starts at `at`.
fn after_string(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

/// The position after the character literal at `at`, or after its quote for a lifetime.
fn after_quote(bytes: &[u8], at: usize) -> usize {
    if bytes.get(at + 1) == Some(&b'\\') {
        return bytes
            .get(at + 3..)
            .and_then(|rest| rest.iter().position(|&byte| byte == b'\''))
            .map_or(bytes.len(), |close| at + 3 + close + 1);
    }
    let Some(&lead) = bytes.get(at + 1) else {
        return bytes.len();
    };
    let width = match lead.leading_ones() {
        0 => 1,
        ones => ones as usize,
    };
    if bytes.get(at + 1 + width) == Some(&b'\'') {
        at + 1 + width + 1
    } else {
        at + 1
    }
}

/// Whether the `r` at `at` begins a raw string, as `r"`, `r#"`, `br"` or `br#"`.
fn starts_raw_string(bytes: &[u8], at: usize) -> bool {
    let prefix_start = if at > 0 && bytes[at - 1] == b'b' {
        at - 1
    } else {
        at
    };
    if prefix_start > 0 && is_identifier(bytes[prefix_start - 1]) {
        return false;
    }
    let hashes = bytes[at + 1..]
        .iter()
        .take_while(|&&byte| byte == b'#')
        .count();
    bytes.get(at + 1 + hashes) == Some(&b'"')
}

/// The position after the raw string whose hashes start at `at`.
fn after_raw_string(bytes: &[u8], at: usize) -> usize {
    let hashes = bytes[at..].iter().take_while(|&&byte| byte == b'#').count();
    let mut close = vec![b'"'];
    close.extend(std::iter::repeat_n(b'#', hashes));
    let body = at + hashes + 1;
    bytes[body..]
        .windows(close.len())
        .position(|window| window == close.as_slice())
        .map_or(bytes.len(), |end| body + end + close.len())
}

#[cfg(test)]
mod tests {
    use crate::ledger::tree::sources::SourceFile;

    use super::{code_of, without_test_items};

    /// A source file at `path` holding `text`.
    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            rel_path: path.to_owned(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_test_target_ships_nothing() {
        assert_eq!(
            code_of(&file("crates/a/tests/budgets.rs", "fn a() {}")),
            None
        );
        assert_eq!(code_of(&file("crates/a/benches/b.rs", "fn a() {}")), None);
    }

    #[test]
    fn a_unit_test_module_is_cut_off_the_end_of_a_shipped_file() {
        let source = file(
            "crates/a/src/lib.rs",
            "fn ship() {}\n\n#[cfg(test)]\nmod tests {\n    fn pretend() {}\n}\n",
        );
        let code = code_of(&source).expect("a shipped file");
        assert!(code.contains("ship"));
        assert!(
            !code.contains("pretend"),
            "a counter moved only by a unit test is moved by the test and not by the framework"
        );
    }

    #[test]
    fn a_shipped_file_with_no_test_module_is_kept_whole() {
        let source = file("crates/a/src/lib.rs", "fn ship() {}\n");
        assert_eq!(code_of(&source).as_deref(), Some("fn ship() {}\n"));
    }

    #[test]
    fn a_test_only_method_hides_nothing_below_it() {
        let code = without_test_items(
            "impl Index {\n    #[cfg(test)]\n    fn rounded(&self) -> Vec<u8> {\n        \
             bump(Counter::Pretend);\n        vec![]\n    }\n\n    fn update(&mut self) {\n        \
             bump(Counter::Shipped);\n    }\n}\n",
        );
        assert!(!code.contains("Pretend"), "{code}");
        assert!(code.contains("bump(Counter::Shipped)"), "{code}");
        assert!(code.ends_with("}\n}\n"), "{code}");
    }

    #[test]
    fn a_test_module_declared_first_hides_nothing_below_it() {
        let code = without_test_items(
            "#[cfg(test)]\nmod tests;\n\n#[cfg(test)]\nuse std::{fmt, io};\n\nfn ship() {}\n",
        );
        assert!(!code.contains("tests"), "{code}");
        assert!(!code.contains("fmt"), "{code}");
        assert!(code.contains("fn ship() {}"), "{code}");
    }

    #[test]
    fn each_test_only_field_and_statement_is_cut_alone() {
        let code = without_test_items(
            "struct Probe {\n    #[cfg(test)]\n    pub(crate) asked: Vec<(u8, Asked)>,\n    kept: u8,\n}\n\
             fn run(&mut self) {\n    #[cfg(test)]\n    self.asked.push(Asked::Open);\n    \
             bump(Counter::Shipped);\n}\n",
        );
        assert!(!code.contains("asked"), "{code}");
        assert!(code.contains("kept: u8,"), "{code}");
        assert!(code.contains("bump(Counter::Shipped)"), "{code}");
    }

    #[test]
    fn a_test_only_item_carrying_more_attributes_is_cut_whole() {
        let code = without_test_items(
            "#[cfg(test)]\n/// A helper.\n#[derive(Debug, Clone)]\npub(crate) struct Helper<A, B> \
             where A: Clone, B: Clone {\n    a: A,\n    b: B,\n}\nfn ship() {}\n",
        );
        assert_eq!(code, "\nfn ship() {}\n");
    }

    #[test]
    fn a_test_only_statement_ends_at_its_semicolon() {
        let code = without_test_items(
            "fn run() {\n    #[cfg(test)]\n    let held = if ready { 1 } else { \
             bump(Counter::Pretend) };\n    bump(Counter::Shipped);\n}\n\
             #[cfg(test)]\nconst fn helper() {}\nfn ship() {}\n",
        );
        assert!(!code.contains("Pretend"), "{code}");
        assert!(!code.contains("helper"), "{code}");
        assert!(code.contains("bump(Counter::Shipped)"), "{code}");
        assert!(code.contains("fn ship() {}"), "{code}");
    }

    #[test]
    fn arrows_and_comparisons_in_a_listed_item_end_it_at_its_comma() {
        let code = without_test_items(
            "struct Probe {\n    #[cfg(test)]\n    asked: fn(u8) -> Vec<u8>,\n    kept: u8,\n}\n\
             fn run(x: u8, y: u8) {\n    match x {\n        #[cfg(test)]\n        \
             0 if x < y => bump(Counter::Pretend),\n        _ => bump(Counter::Shipped),\n    }\n}\n",
        );
        assert!(!code.contains("asked"), "{code}");
        assert!(code.contains("kept: u8,"), "{code}");
        assert!(!code.contains("Pretend"), "{code}");
        assert!(code.contains("bump(Counter::Shipped)"), "{code}");
    }

    #[test]
    fn delimiters_inside_comments_and_literals_do_not_end_the_item() {
        let code = without_test_items(
            "#[cfg(test)]\nfn pretend<'a>(_: &'a str) {\n    let _ = ('}', \"}\", r#\"}\"#);\n    \
             // }\n    /* } */\n    bump(Counter::Pretend);\n}\nfn ship() {}\n",
        );
        assert_eq!(code, "\nfn ship() {}\n");
    }

    #[test]
    fn the_attribute_named_in_a_comment_or_a_string_cuts_nothing() {
        let text = "/// Behind `#[cfg(test)]` only.\nconst NAME: &str = \"#[cfg(test)]\";\n\
                    fn ship() {}\n";
        assert_eq!(without_test_items(text), text);
    }
}
