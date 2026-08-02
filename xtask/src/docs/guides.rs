//! The guides, and the two things that make a set of them a set.
//!
//! A guide is only useful if it is there and if its cross-references resolve, so both are checked
//! rather than noticed by a reader. The same forbidden-phrasing rules apply here as to
//! documentation attached to items: a guide that defers to the repository's working notes is a
//! guide that stops being true without anything failing.

use std::collections::BTreeSet;
use std::path::Path;

use crate::docs::forbidden::RULES;
use crate::docs::sources;
use crate::error::{Result, read_to_string};
use crate::ledger::report::Report;

/// The guides that must exist, and what each one answers.
const REQUIRED: [(&str, &str); 6] = [
    ("architecture.md", "what the framework is made of"),
    ("layering.md", "what may depend on what"),
    (
        "browser.md",
        "the extension points a document language needs",
    ),
    ("renderer.md", "how to write a second renderer"),
    ("styling.md", "how styling works and what a change costs"),
    (
        "reactivity.md",
        "the reactive model and the escapes from its thread-safety bounds",
    ),
];

/// Checks the guide directory.
pub(crate) fn check(root: &Path) -> Result<Report> {
    let mut report = Report::clean();
    let directory = root.join("docs").join("guide");
    let files = sources::markdown_files(&directory)?;
    let present: BTreeSet<String> = files
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();

    for (name, answers) in REQUIRED {
        if !present.contains(name) {
            report.violation(
                format!("docs/guide/{name}"),
                format!("missing: the guide answering {answers}"),
            );
        }
    }

    for path in &files {
        let text = read_to_string(path)?;
        let relative = sources::relative(root, path);
        if text.trim().is_empty() {
            report.violation(relative.clone(), "empty".to_owned());
            continue;
        }
        for (number, line) in text.lines().enumerate() {
            for rule in RULES {
                if (rule.matches)(line) {
                    report.violation(
                        format!("{relative}:{}", number + 1),
                        format!("{}: {} — {}", rule.name, line.trim(), rule.instead),
                    );
                }
            }
        }
        for target in links(&text) {
            if !directory.join(&target).exists() {
                report.violation(
                    relative.clone(),
                    format!("link to `{target}` resolves to no file"),
                );
            }
        }
    }

    if files.is_empty() {
        report.skip("no guides to read".to_owned());
    } else {
        println!("    read {} guides", files.len());
    }
    Ok(report)
}

/// Every relative link to another markdown file in `text`.
///
/// Absolute links and links to anything but a sibling document are somebody else's to check.
fn links(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("](") {
        rest = &rest[open + 2..];
        let Some(close) = rest.find(')') else { break };
        let target = &rest[..close];
        rest = &rest[close + 1..];
        let target = target.split('#').next().unwrap_or_default();
        if target.ends_with(".md") && !target.contains("://") && !target.starts_with('/') {
            found.push(target.to_owned());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::links;

    /// Sibling documents are collected; anchors are trimmed and foreign links are left alone.
    #[test]
    fn only_sibling_documents_are_collected() {
        let text = "see [a](layering.md), [b](styling.md#cost), [c](https://example.invalid/x.md)";
        assert_eq!(
            links(text),
            vec!["layering.md".to_owned(), "styling.md".to_owned()]
        );
    }
}
