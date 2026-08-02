//! The wall-clock ledger.
//!
//! An assertion about elapsed time is worth nothing unless something runs it, and the ways it ends
//! up never being run are quiet ones. It can be gated on the build profile, so the unoptimised gate
//! skips it and the optimised job that would have executed it exists only in a workflow file nobody
//! runs. It can sit in a target nothing selects. Either way it stays green for ever, it names the
//! right quantity, and the number in it stopped being true long ago.
//!
//! So a wall-clock assertion has exactly one place to live. The crate puts it in a test target
//! named `wall_clock`, declared with `required-features = ["wall-clock"]` so that an unoptimised
//! run cannot execute it and read an unoptimised time as a regression, and names itself in the gate
//! runner's list so that the gate runs that target in release. This check holds the three together:
//! a timing assertion in any other test file, a target the gate has not been told about, and a name
//! in the gate's list that points at no target.
//!
//! What it reads is every file under a member's `tests/`, which is where a crate's own suites live.
//! A `#[cfg(test)] mod tests` inside `src/` is deliberately outside its reach and is not covered by
//! anything: the check would have to be able to tell an assertion in a shipped module from one in
//! a test module beside it, and reading Rust that closely is not what a text ledger is for.
//!
//! The check is about *where the assertion is*, not about how it was written. That is deliberate:
//! `if !cfg!(debug_assertions) { assert!(elapsed < …) }` in an ordinary test file is caught here as
//! an assertion in the wrong file, without this having to understand the condition — while a test
//! that is legitimately compiled for one profile only, such as one proving that something compiles
//! out of an optimised build, is not caught at all.

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::member::Member;

/// The test target every wall-clock budget lives in.
const TARGET: &str = "wall_clock";

/// The feature that target is behind.
const FEATURE: &str = "wall-clock";

/// The gate runner: the file that says which crates own such a target and runs their targets.
const RUNNER: &str = "/src/wall_clock.rs";

/// The list inside it.
const LIST: &str = "WALL_CLOCK_MEMBERS: &[&str]";

/// How the real clock is read.
///
/// A test that never reads it cannot assert on a wall-clock time, whatever else it does with a
/// duration: a harness whose clock the test itself advances is a different thing entirely, and its
/// assertions are exact on every machine.
const REAL_CLOCK: &str = "Instant::now()";

/// The forms an elapsed time is compared as.
const ELAPSED: [&str; 6] = [
    ".elapsed()",
    "Duration::from",
    "as_secs",
    "as_millis",
    "as_micros",
    "as_nanos",
];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let listed = gate_list(tree);

    for member in &tree.members {
        let target = format!("{}/tests/{TARGET}.rs", member.rel_dir);
        let prefix = format!("{}/tests/", member.rel_dir);
        for file in &member.sources {
            if !file.rel_path.starts_with(&prefix) || file.rel_path == target {
                continue;
            }
            for line in wall_clock_assertions(&file.text) {
                report.violation(
                    file.rel_path.clone(),
                    format!(
                        "line {line} asserts on a time read from the real clock, and nothing runs \
                         this file in an optimised build: move it into `{target}`, which the gate \
                         runs in release"
                    ),
                );
            }
        }

        if !member.sources.iter().any(|file| file.rel_path == target) {
            continue;
        }
        for message in declaration(member) {
            report.violation(member.manifest.rel_path.clone(), message);
        }
        if listed
            .as_ref()
            .is_some_and(|names| !names.contains(&member.name))
        {
            report.violation(
                target,
                format!(
                    "`{}` owns a `{TARGET}` target and the gate's `{LIST}` does not name it, so \
                     the gate never runs it",
                    member.name
                ),
            );
        }
    }

    match listed {
        None => report.skip(format!("no crate in this tree declares `{LIST}`")),
        Some(names) => {
            for name in names {
                if !owns_target(tree, &name) {
                    report.violation(
                        LIST.to_owned(),
                        format!(
                            "the gate is told to run `{name}`'s `{TARGET}` target and there is no \
                             such target, so that step measures nothing"
                        ),
                    );
                }
            }
        }
    }
    report
}

/// Whether the member called `name` owns a wall-clock target.
fn owns_target(tree: &Tree, name: &str) -> bool {
    tree.members.iter().any(|member| {
        member.name == name
            && member
                .sources
                .iter()
                .any(|file| file.rel_path == format!("{}/tests/{TARGET}.rs", member.rel_dir))
    })
}

/// The crates the gate runner is told to run wall-clock targets for.
///
/// Found by path as well as by name, because this file states the same declaration in prose and
/// would otherwise read the list out of itself and find it empty.
fn gate_list(tree: &Tree) -> Option<Vec<String>> {
    let file = tree
        .members
        .iter()
        .flat_map(|member| &member.sources)
        .find(|file| file.rel_path.ends_with(RUNNER) && file.text.contains(LIST))?;
    let list = file.text.split(LIST).nth(1)?.split(']').next()?;
    Some(
        list.split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect(),
    )
}

/// What is wrong with the way a member declares its wall-clock target, if anything.
///
/// Two halves, and both are load-bearing. The feature is what keeps an unoptimised run from
/// executing a budget, and the target declaration is what puts the target behind the feature.
fn declaration(member: &Member) -> Vec<String> {
    let mut out = Vec::new();
    let declares_feature = member
        .manifest
        .table
        .get("features")
        .and_then(|features| features.as_table())
        .is_some_and(|features| features.contains_key(FEATURE));
    if !declares_feature {
        out.push(format!(
            "owns a `{TARGET}` test target and declares no `{FEATURE}` feature"
        ));
    }
    let gated = member
        .manifest
        .table
        .get("test")
        .and_then(|tests| tests.as_array())
        .is_some_and(|tests| tests.iter().any(is_gated_target));
    if !gated {
        out.push(format!(
            "the `{TARGET}` test target must be declared with `required-features = \
             [\"{FEATURE}\"]`, or an unoptimised run executes it and reads an unoptimised time as \
             a regression"
        ));
    }
    out
}

/// Whether one `[[test]]` entry is the wall-clock target, behind its feature.
fn is_gated_target(entry: &toml::Value) -> bool {
    entry.get("name").and_then(toml::Value::as_str) == Some(TARGET)
        && entry
            .get("required-features")
            .and_then(toml::Value::as_array)
            .is_some_and(|features| {
                features
                    .iter()
                    .any(|feature| feature.as_str() == Some(FEATURE))
            })
}

/// The lines at which `text` asserts on a time read from the real clock.
///
/// An assertion is read as far as the bracket that closes it rather than to the end of its first
/// line, because the interesting ones are written over several lines and the comparison is never on
/// the same line as the word `assert`. Recording a duration is not asserting on one: a case that
/// prints what it measured and asserts only on what it did is a measurement no machine can turn
/// red, and this leaves it alone.
fn wall_clock_assertions(text: &str) -> Vec<usize> {
    if !text.contains(REAL_CLOCK) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (offset, _) in text.match_indices("assert") {
        // `debug_assertions` ends in the word and is not an assertion.
        if text[..offset]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || before == '_')
        {
            continue;
        }
        let invocation = invocation(&text[offset..]);
        if ELAPSED.iter().any(|form| invocation.contains(form)) {
            out.push(text[..offset].matches('\n').count() + 1);
        }
    }
    out
}

/// One macro invocation, from its name to the bracket that closes its arguments.
fn invocation(text: &str) -> &str {
    let Some(open) = text.find(['(', '[', '{']) else {
        return text;
    };
    let mut depth = 0_u32;
    for (offset, character) in text[open..].char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[..open + offset];
                }
            }
            _ => {}
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::wall_clock_assertions;

    /// The text of a file that reads the real clock somewhere, with `body` after it.
    fn file(body: &str) -> String {
        format!("let start = Instant::now();\n{body}")
    }

    #[test]
    fn an_assertion_spread_over_several_lines_is_still_one_about_a_time() {
        let text = file(
            "let elapsed = start.elapsed();\nassert!(\n    elapsed < Duration::from_millis(2),\n \
             \"too slow\"\n);\n",
        );
        assert_eq!(wall_clock_assertions(&text), vec![3]);
    }

    #[test]
    fn recording_a_time_beside_an_assertion_about_something_else_is_not_a_budget() {
        let text = file(
            "assert_eq!(pass.workers, 1, \"the pool was used\");\n(pass.styled, \
             start.elapsed())\n",
        );
        assert!(wall_clock_assertions(&text).is_empty());
    }

    #[test]
    fn the_word_inside_a_longer_name_is_not_an_assertion() {
        // `cfg!(debug_assertions)` sits beside the assertion it gates, and reporting it as well
        // would name the same defect twice at two different lines.
        let text = file(
            "let elapsed = start.elapsed();\nif !cfg!(debug_assertions) {\n                 assert!(elapsed.as_secs_f64() < 0.002);\n}\n",
        );
        assert_eq!(wall_clock_assertions(&text), vec![4]);
    }

    #[test]
    fn a_duration_compared_against_a_clock_the_test_advances_itself_is_not_a_budget() {
        // Nothing in the file reads the real clock, so the comparison is exact and reproducible
        // whatever else the machine is doing.
        let text = "assert_eq!(harness.now(), start + Duration::from_millis(5));\n";
        assert!(wall_clock_assertions(text).is_empty());
    }
}
