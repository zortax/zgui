//! Which members may turn the counters on for everything that depends on them.
//!
//! Cargo unifies features over the build graph, so one normal dependency on the profile crate with
//! `counters` on compiles the counters into every application above it. A member that reads the
//! counters in a test names the feature in `[dev-dependencies]`, where it reaches only the member's
//! own test and bench builds.

use crate::ledger::report::Report;
use crate::ledger::tree::manifest::{Manifest, Section};

/// The crate that defines the counters.
const PROFILE: &str = "zgui-profile";

/// The feature that compiles them into an optimised build.
const FEATURE: &str = "counters";

/// The members that exist to read the counters, and so turn them on as a normal dependency.
///
/// Only test and measurement tools: nothing an application links to run. Only test tooling may
/// depend on them, because the check does not follow a dependency on a reader.
const READERS: &[&str] = &["zgui-bench", "zgui-testkit-scene"];

/// Records a violation for each normal dependency in `manifest` that turns the counters on.
pub(crate) fn check(member: &str, manifest: &Manifest, report: &mut Report) {
    if READERS.contains(&member) {
        return;
    }
    for dependency in manifest.dependencies() {
        if dependency.section == Section::Normal
            && dependency.name == PROFILE
            && dependency.features.iter().any(|feature| feature == FEATURE)
        {
            report.violation(
                manifest.rel_path.clone(),
                format!(
                    "`{member}` turns on `{PROFILE}/{FEATURE}` as a normal dependency, which \
                     compiles the counters into every release application above it. Name the \
                     feature in `[dev-dependencies]`, or behind a feature of `{member}`."
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::check;
    use crate::ledger::report::Report;
    use crate::ledger::tree::manifest::Manifest;

    /// A manifest parsed from `text`.
    fn manifest(text: &str) -> Manifest {
        Manifest {
            rel_path: "crates/a/Cargo.toml".to_owned(),
            text: text.to_owned(),
            table: text.parse().expect("a manifest"),
        }
    }

    /// The violations `check` reports for `member` with the manifest `text`.
    fn violations(member: &str, text: &str) -> usize {
        let mut report = Report::clean();
        check(member, &manifest(text), &mut report);
        report.violations.len()
    }

    const NORMAL: &str = "[dependencies]\n\
        zgui-profile = { path = \"../zgui-profile\", features = [\"counters\"] }\n";

    #[test]
    fn a_normal_dependency_with_the_counters_is_refused() {
        assert_eq!(violations("zgui-runtime", NORMAL), 1);
    }

    #[test]
    fn a_target_specific_normal_dependency_is_refused_too() {
        let text = "[target.'cfg(unix)'.dependencies]\n\
            zgui-profile = { path = \"../zgui-profile\", features = [\"counters\"] }\n";
        assert_eq!(violations("zgui-runtime", text), 1);
    }

    #[test]
    fn a_dev_dependency_with_the_counters_is_accepted() {
        let text = "[dependencies]\n\
            zgui-profile = { path = \"../zgui-profile\" }\n\
            [dev-dependencies]\n\
            zgui-profile = { path = \"../zgui-profile\", features = [\"counters\"] }\n";
        assert_eq!(violations("zgui-runtime", text), 0);
    }

    #[test]
    fn a_member_that_exists_to_read_the_counters_is_accepted() {
        assert_eq!(violations("zgui-bench", NORMAL), 0);
        assert_eq!(violations("zgui-testkit-scene", NORMAL), 0);
    }
}
