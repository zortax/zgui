//! One version for the whole tree.
//!
//! Every crate here is released together at one version. That is a decision about what a consumer
//! has to reason about: with it, `zgui 0.1.0` names exactly one build of everything underneath, and
//! a compatibility matrix between our own crates never comes into existence.
//!
//! Two things have to hold for it, and neither of them is visible until a release is attempted.
//! A member that spells its own version instead of inheriting the workspace's drifts silently. And
//! an internal dependency written as a bare path publishes to no registry at all: a path is a fact
//! about this checkout, so a package whose dependency has no version requirement beside the path is
//! rejected the moment somebody runs the publish.

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::manifest::{Manifest, Section};

/// Checks that every member shares the workspace version and can be published against it.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let Some(version) = workspace_version(&tree.manifest) else {
        report.skip("the workspace manifest declares no `[workspace.package] version`".to_owned());
        return report;
    };

    let members: Vec<&str> = tree
        .members
        .iter()
        .map(|member| member.name.as_str())
        .collect();

    for member in &tree.members {
        if !inherits_version(&member.manifest) {
            report.violation(
                member.manifest.rel_path.clone(),
                "write `version.workspace = true`: every crate here is released at one version"
                    .to_owned(),
            );
        }
        if !member.manifest.is_published() {
            continue;
        }
        for dependency in member.manifest.dependencies() {
            // A development dependency is stripped from the manifest a registry receives, so a
            // bare path is legal there and only there.
            if dependency.section == Section::Dev {
                continue;
            }
            if !members.contains(&dependency.name.as_str()) {
                continue;
            }
            match dependency.version.as_deref() {
                None => report.violation(
                    member.manifest.rel_path.clone(),
                    format!(
                        "`{}` is a bare path dependency: add `version = \"{version}\"` beside the \
                         path, or the crate cannot be published",
                        dependency.name
                    ),
                ),
                Some(declared) if declared != version => report.violation(
                    member.manifest.rel_path.clone(),
                    format!(
                        "`{}` asks for `{declared}` while the workspace is at `{version}`: the \
                         tree is released in lockstep",
                        dependency.name
                    ),
                ),
                Some(_) => {}
            }
        }
    }
    report
}

/// The `[workspace.package] version` every member inherits.
pub(crate) fn workspace_version(manifest: &Manifest) -> Option<String> {
    manifest
        .workspace_table("package")?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

/// Whether the member inherits its version rather than spelling one.
fn inherits_version(manifest: &Manifest) -> bool {
    manifest
        .text
        .lines()
        .any(|line| line.trim() == "version.workspace = true")
}
