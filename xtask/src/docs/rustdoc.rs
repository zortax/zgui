//! The documentation attached to Rust items, and what it may not say.

use std::path::Path;

use crate::docs::forbidden::RULES;
use crate::docs::sources;
use crate::error::{Result, read_to_string};
use crate::ledger::report::Report;

/// Checks every documentation comment under `crates/`.
pub(crate) fn check(root: &Path) -> Result<Report> {
    let mut report = Report::clean();
    let mut files = 0;
    let mut lines = 0;
    for path in sources::rust_files(&root.join("crates"))? {
        files += 1;
        let text = read_to_string(&path)?;
        let relative = sources::relative(root, &path);
        for (number, line) in text.lines().enumerate() {
            if !is_documentation(line) {
                continue;
            }
            lines += 1;
            for rule in RULES {
                if (rule.matches)(line) {
                    report.violation(
                        format!("{relative}:{}", number + 1),
                        format!("{}: {} — {}", rule.name, line.trim(), rule.instead),
                    );
                }
            }
        }
    }
    if files == 0 {
        report.skip("no crate sources to read".to_owned());
    } else {
        println!("    read {lines} documentation lines in {files} files");
    }
    Ok(report)
}

/// Whether `line` is documentation rather than code or an ordinary comment.
///
/// Both comment forms count, and so does the attribute form, which is what a macro that generates
/// documentation writes.
fn is_documentation(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with("#[doc = ")
        || trimmed.starts_with("#![doc = ")
}

#[cfg(test)]
mod tests {
    use super::is_documentation;

    /// Documentation is recognised in every form, and code is not documentation.
    #[test]
    fn documentation_is_told_from_code() {
        assert!(is_documentation("/// An item."));
        assert!(is_documentation("    //! A module."));
        assert!(is_documentation("    #[doc = \"generated\"]"));
        assert!(!is_documentation("// an ordinary comment"));
        assert!(!is_documentation("let phase = 27;"));
    }
}
