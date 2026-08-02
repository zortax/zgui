//! Reading the counter set out of the crate that declares it.

use crate::ledger::check::counters::shipped;
use crate::ledger::tree::Tree;

/// The macro invocation the counter set is written inside.
const INVOCATION: &str = "counters! {";

/// The separator between a counter's variant name and its field name.
const ARROW: &str = " => ";

/// The counter set, and where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declaration {
    /// The member that declares the counters, which is therefore never a producer of them.
    pub(crate) member: String,
    /// The file the set was read from, relative to the tree root.
    pub(crate) file: String,
    /// Every counter's variant name, in declaration order.
    pub(crate) counters: Vec<String>,
}

/// Finds the counter set in `tree`, if it has one.
pub(crate) fn find(tree: &Tree) -> Option<Declaration> {
    for member in &tree.members {
        for file in &member.sources {
            let Some(code) = shipped::code_of(file) else {
                continue;
            };
            if !code.contains(INVOCATION) {
                continue;
            }
            let counters = names(code);
            if !counters.is_empty() {
                return Some(Declaration {
                    member: member.name.clone(),
                    file: file.rel_path.clone(),
                    counters,
                });
            }
        }
    }
    None
}

/// Every counter name declared in `text`.
fn names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.trim().split_once(ARROW)?;
            // A doc comment mentioning the syntax is not a declaration, and neither is anything
            // that does not end the statement.
            if name.starts_with("//") || !rest.trim_end().ends_with(';') {
                return None;
            }
            let name = name.trim();
            name.chars()
                .next()
                .is_some_and(char::is_uppercase)
                .then(|| name.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::names;

    #[test]
    fn reads_the_variant_names_and_nothing_else() {
        let text = "\
counters! {
    /// A counter.
    Alpha => alpha, Group::BackendNeutral;

    /// Another, whose doc says `Beta => beta` without declaring it
    Gamma => gamma, Group::RendererSpecific;
}
";
        assert_eq!(names(text), vec!["Alpha".to_owned(), "Gamma".to_owned()]);
    }

    #[test]
    fn a_file_with_no_declarations_yields_none() {
        assert!(names("fn main() {}\n").is_empty());
    }
}
