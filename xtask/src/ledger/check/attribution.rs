//! The attribution ledger.
//!
//! Adapted code carries a header naming its origin and licence, and `NOTICE` carries a matching
//! row. Both directions are checked, because a header nobody records and a record nobody wrote
//! fail in opposite ways. Copyleft headers are refused outright outside `vendor/`.

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// The marker that makes a file a derived file.
const MARKER: &str = "DERIVED-FROM:";

/// The licence texts a derived file may carry, and the identifier its header must name.
const LICENCES: [(&str, &str); 2] = [
    ("Apache-2.0", "Apache License, Version 2.0"),
    ("MIT", "MIT License"),
];

/// The `NOTICE` heading whose rows list the derived files.
const NOTICE_SECTION: &str = "## Derived files";

/// Licence families that may not appear in a workspace source file at all.
const REFUSED: [&str; 2] = ["Mozilla Public License", "GNU General Public License"];

/// The member that defines the markers above, and would otherwise trip on its own vocabulary.
const EXEMPT: [&str; 1] = ["xtask"];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let recorded = notice_rows(&tree.notice);

    let mut found = Vec::new();
    for member in &tree.members {
        if EXEMPT.contains(&member.name.as_str()) {
            continue;
        }
        for file in &member.sources {
            for refused in REFUSED {
                if file.text.contains(refused) {
                    report.violation(
                        file.rel_path.clone(),
                        format!("carries a {refused} header, which no crate here may contain"),
                    );
                }
            }
            let Some(header) = file.text.lines().find(|line| line.contains(MARKER)) else {
                continue;
            };
            found.push(file.rel_path.clone());

            let identifier = LICENCES
                .iter()
                .find(|(identifier, _)| header.contains(identifier));
            match identifier {
                Some((identifier, full_text)) => {
                    if !file.text.contains(full_text) {
                        report.violation(
                            file.rel_path.clone(),
                            format!(
                                "declares `{identifier}` but does not reproduce the \"{full_text}\" notice"
                            ),
                        );
                    }
                }
                None => report.violation(
                    file.rel_path.clone(),
                    format!(
                        "`{MARKER}` header names no licence; write one of {}",
                        LICENCES
                            .iter()
                            .map(|(identifier, _)| *identifier)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ),
            }

            if !recorded.contains(&file.rel_path) {
                report.violation(
                    file.rel_path.clone(),
                    format!("is derived code with no row under `{NOTICE_SECTION}` in NOTICE"),
                );
            }
        }
    }

    for row in &recorded {
        if !found.contains(row) {
            report.violation(
                "NOTICE",
                format!("lists `{row}`, which carries no `{MARKER}` header"),
            );
        }
    }
    report
}

/// The file paths recorded in `NOTICE`'s derived-files table.
fn notice_rows(notice: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut in_section = false;
    for line in notice.lines() {
        if line.starts_with("## ") {
            in_section = line.trim() == NOTICE_SECTION;
            continue;
        }
        if !in_section || !line.trim_start().starts_with('|') {
            continue;
        }
        let Some(cell) = line.split('|').nth(1) else {
            continue;
        };
        let cell = cell.trim();
        if let Some(path) = cell
            .strip_prefix('`')
            .and_then(|rest| rest.strip_suffix('`'))
        {
            rows.push(path.to_owned());
        }
    }
    rows
}
