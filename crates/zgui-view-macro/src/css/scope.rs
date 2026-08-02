//! Scoping a component's sheet to a class nothing else can collide with.

/// The class a scoped sheet is written against.
///
/// The name is derived from the component's name and the text of its rules, so it is the same on
/// every build of the same source and different the moment the sheet changes.
pub(crate) fn class_of(name: &str, css: &str) -> String {
    format!("zs-{:08x}", hash(name, css))
}

/// Rewrites `:scope` to the sheet's own class, leaving strings and comments alone.
pub(crate) fn rewrite(css: &str, class: &str) -> String {
    let selector = format!(".{class}");
    let characters: Vec<char> = css.chars().collect();
    let mut out = String::with_capacity(css.len());
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '/' && characters.get(index + 1) == Some(&'*') {
            let start = index;
            index += 2;
            while index + 1 < characters.len()
                && !(characters[index] == '*' && characters[index + 1] == '/')
            {
                index += 1;
            }
            index = (index + 2).min(characters.len());
            out.extend(&characters[start..index]);
            continue;
        }
        if character == '"' || character == '\'' {
            let start = index;
            index += 1;
            while index < characters.len() && characters[index] != character {
                index += usize::from(characters[index] == '\\') + 1;
            }
            index = (index + 1).min(characters.len());
            out.extend(&characters[start..index]);
            continue;
        }
        if character == ':' && starts_with(&characters, index, ":scope") {
            out.push_str(&selector);
            index += ":scope".len();
            continue;
        }
        out.push(character);
        index += 1;
    }
    out
}

/// Whether the text at `index` is `needle`.
fn starts_with(characters: &[char], index: usize, needle: &str) -> bool {
    needle
        .chars()
        .enumerate()
        .all(|(offset, expected)| characters.get(index + offset) == Some(&expected))
}

/// FNV-1a over the name and the text, which is stable across builds and platforms.
fn hash(name: &str, css: &str) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in name.bytes().chain(b"=>".iter().copied()).chain(css.bytes()) {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{class_of, rewrite};

    #[test]
    fn the_class_follows_the_name_and_the_text() {
        let first = class_of("Button", ":scope { color: red }");
        assert_eq!(first, class_of("Button", ":scope { color: red }"));
        assert_ne!(first, class_of("Card", ":scope { color: red }"));
        assert_ne!(first, class_of("Button", ":scope { color: blue }"));
        assert!(first.starts_with("zs-"));
    }

    #[test]
    fn every_scope_becomes_the_class() {
        assert_eq!(
            rewrite(":scope:hover :scope > * { color: red }", "zs-1"),
            ".zs-1:hover .zs-1 > * { color: red }"
        );
    }

    #[test]
    fn a_scope_inside_a_string_or_a_comment_is_left_alone() {
        assert_eq!(
            rewrite("/* :scope */ a::before { content: \":scope\" }", "zs-1"),
            "/* :scope */ a::before { content: \":scope\" }"
        );
    }
}
