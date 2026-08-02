//! The spikes ledger.
//!
//! A spike is a running program that answers one question and then dies. Every spike names the
//! phase that absorbs its findings and deletes it, and a spike that is still here once the tree
//! has been built past that phase is a spike that became infrastructure by accident.

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// The header a spike manifest carries.
const HEADER: &str = "# RETIRE: phase ";

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let reached = tree.phases.reached_phase(
        tree.members
            .iter()
            .filter(|member| !member.is_spike())
            .map(|member| member.name.as_str()),
    );

    for member in tree.members.iter().filter(|member| member.is_spike()) {
        let Some(retire) = retire_phase(&member.manifest.text) else {
            report.violation(
                member.manifest.rel_path.clone(),
                format!("a spike manifest must carry a `{HEADER}NN` header naming the phase that deletes it"),
            );
            continue;
        };
        if reached > retire {
            report.violation(
                member.manifest.rel_path.clone(),
                format!(
                    "retires in phase {retire}, but the tree has been built out to phase {reached}: delete it"
                ),
            );
        }
    }
    report
}

/// The phase named by the manifest's retire header.
fn retire_phase(manifest: &str) -> Option<u32> {
    let line = manifest.lines().find(|line| line.contains(HEADER))?;
    let rest = line.split(HEADER).nth(1)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::retire_phase;

    #[test]
    fn reads_the_header() {
        assert_eq!(retire_phase("# RETIRE: phase 27\n[package]\n"), Some(27));
        assert_eq!(retire_phase("[package]\n"), None);
    }
}
