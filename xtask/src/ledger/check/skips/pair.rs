//! Reading the declared skip pairs out of the counter table.

/// The separator between a counter's variant name and its field name.
const ARROW: &str = " => ";

/// How a skip declares itself.
const SKIP: &str = "Group::Skip";

/// The field of that declaration naming the counter of work performed.
const DONE: &str = "done: Counter::";

/// One counter of avoided work, and the counter of performed work it is read against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pair {
    /// The counter that records work the stage did not do.
    pub(crate) skipped: String,
    /// The counter that records the work it did instead, or `None` when the declaration named
    /// nothing.
    pub(crate) done: Option<String>,
}

/// Every skip pair declared in `text`.
pub(crate) fn pairs(text: &str) -> Vec<Pair> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with("//") {
                return None;
            }
            let (skipped, rest) = line.split_once(ARROW)?;
            let rest = rest.trim_end();
            if !rest.ends_with(';') || !rest.contains(SKIP) {
                return None;
            }
            let skipped = skipped.trim();
            if !skipped.chars().next().is_some_and(char::is_uppercase) {
                return None;
            }
            Some(Pair {
                skipped: skipped.to_owned(),
                done: named(rest),
            })
        })
        .collect()
}

/// The counter `DONE` names in a declaration, if it names one.
fn named(declaration: &str) -> Option<String> {
    let at = declaration.find(DONE)? + DONE.len();
    let name: String = declaration[at..]
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::{Pair, pairs};

    /// The declaration form the counter table uses.
    const TABLE: &str = "\
counters! {
    /// A plain count of work performed.
    Alpha => alpha, Group::BackendNeutral;

    /// A count of work avoided, read against Alpha.
    Beta => beta, Group::Skip { done: Counter::Alpha };

    /// A doc comment mentioning `Gamma => gamma, Group::Skip { done: Counter::Alpha }`.
    Delta => delta, Group::RendererSpecific;
}
";

    #[test]
    fn only_the_skip_declarations_are_read_and_each_names_its_pair() {
        assert_eq!(
            pairs(TABLE),
            vec![Pair {
                skipped: "Beta".to_owned(),
                done: Some("Alpha".to_owned()),
            }]
        );
    }

    #[test]
    fn a_skip_that_names_no_pair_is_still_a_skip() {
        // It has to be, or the gate would answer a malformed declaration by looking away from it.
        let text = "    Beta => beta, Group::Skip { done: };\n";
        assert_eq!(
            pairs(text),
            vec![Pair {
                skipped: "Beta".to_owned(),
                done: None,
            }]
        );
    }

    #[test]
    fn a_declaration_split_over_two_lines_is_not_mistaken_for_one() {
        assert!(
            pairs("    Beta => beta,\n        Group::Skip { done: Counter::Alpha };\n").is_empty()
        );
    }
}
