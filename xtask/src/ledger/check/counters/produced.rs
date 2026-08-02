//! Finding the code that increments a counter.

use std::collections::BTreeMap;

use crate::ledger::check::counters::declared::Declaration;
use crate::ledger::check::counters::shipped;
use crate::ledger::tree::Tree;

/// Where each counter is incremented, by counter name.
pub(crate) type Producers = BTreeMap<String, Vec<String>>;

/// Every counter in `declaration` that some member's shipped code increments.
pub(crate) fn find(tree: &Tree, declaration: &Declaration) -> Producers {
    let mut producers = Producers::new();
    for member in &tree.members {
        // The declaring crate's own doc examples and unit tests increment counters to demonstrate
        // the API. That is not a stage producing work, and counting it would make every counter
        // look produced the moment it was documented.
        if member.name == declaration.member {
            continue;
        }
        for file in &member.sources {
            let Some(text) = shipped::code_of(file) else {
                continue;
            };
            let code = code_of(text);
            for counter in &declaration.counters {
                if increments(&code, counter) {
                    producers
                        .entry(counter.clone())
                        .or_default()
                        .push(file.rel_path.clone());
                }
            }
        }
    }
    producers
}

/// `text` with its comment lines dropped and its whitespace removed.
///
/// Comments go first because a doc example is prose: `counter::add(Counter::Wakes, 4)` inside one
/// shows how to call the API and increments nothing. The whitespace goes because a call whose
/// arguments were wrapped onto their own lines is the same call.
fn code_of(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(|line| line.chars().filter(|character| !character.is_whitespace()))
        .collect()
}

/// Whether `code` hands `counter` to one of the two functions that add to it.
fn increments(code: &str, counter: &str) -> bool {
    // The trailing delimiter is what keeps the search exact: without it `Counter::Wakes` would also
    // be found inside a longer name that happens to start with it.
    code.contains(&format!("bump(Counter::{counter})"))
        || code.contains(&format!("add(Counter::{counter},"))
        || code.contains(&format!("set(Counter::{counter},"))
}

#[cfg(test)]
mod tests {
    use super::{code_of, increments};

    #[test]
    fn a_bump_and_an_add_both_count() {
        let code = code_of("counter::bump(Counter::Wakes);\ncounter::add(Counter::Repaints, 3);\n");
        assert!(increments(&code, "Wakes"));
        assert!(increments(&code, "Repaints"));
    }

    #[test]
    fn a_call_split_across_lines_counts() {
        let code = code_of(
            "counter::add(\n    Counter::ElementsRecascaded,\n    (styled - matched) as u64,\n);\n",
        );
        assert!(increments(&code, "ElementsRecascaded"));
    }

    #[test]
    fn a_mention_that_is_not_an_increment_does_not_count() {
        // Reading a counter, naming it in a match arm or listing it in an array are all ways to
        // mention a counter without ever moving it, and each of them would make a dead counter look
        // alive.
        let code = code_of(
            "let n = counter::get(Counter::Wakes);\nconst ALL: [Counter; 1] = [Counter::Wakes];\n",
        );
        assert!(!increments(&code, "Wakes"));
    }

    #[test]
    fn an_increment_inside_a_comment_does_not_count() {
        let code =
            code_of("//! counter::bump(Counter::Wakes);\n/// counter::bump(Counter::Wakes);");
        assert!(!increments(&code, "Wakes"));
    }

    #[test]
    fn a_longer_name_that_starts_with_a_shorter_one_is_not_confused_for_it() {
        let code = code_of("counter::bump(Counter::StylesLoweredFromCache);");
        assert!(increments(&code, "StylesLoweredFromCache"));
        assert!(
            !increments(&code, "StylesLowered"),
            "the prefix has to stay unproduced, or wiring one counter would silently satisfy \
             another"
        );
    }
}
