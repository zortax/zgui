//! The topological ledger.
//!
//! The schedule is only a schedule if it is buildable in order: a crate introduced in phase *N*
//! may not be depended on by a crate introduced before *N*. Checking it against the real
//! manifest graph is what stops the schedule from becoming fiction.

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// Members with no phase of their own, and why.
///
/// The canary and the gate runner exist from the start and depend on nothing scheduled.
const UNSCHEDULED: [&str; 2] = ["probe", "xtask"];

/// Edges the schedule authorises in the other direction, and the reason each one exists.
///
/// The rule this check enforces — a crate may not depend on one that arrives later — reads the
/// manifests as they are *now* against the phase each crate *arrived* in, and those are two
/// different questions for one shape of edge: something built early that gains a delegate when a
/// later phase lands the thing it delegates to. The frame loop is the first example — it is built
/// before the systems it runs, because a system with no loop to run it cannot be tested at all —
/// and the component harness is the second, for the same reason and with a sharper consequence: a
/// harness that does not perform a framework default is a harness that passes components which do
/// not work in a window. Nothing about either is unbuildable in order, since at the earlier phase
/// the dependency did not exist, but the check cannot see the tree as it was, so the pairs are
/// named here.
///
/// This is not a general escape hatch. Every entry is one edge, from one named crate to one named
/// crate, and each is an edge the architecture states outright.
const DRIVEN: [(&str, &str); 6] = [
    ("zgui-runtime", "zgui-scroll"),
    ("zgui-runtime", "zgui-anim"),
    ("zgui-runtime", "zgui-edit"),
    ("zgui-runtime", "zgui-a11y"),
    // Typing is a framework default, so the harness that drives a component has to perform it,
    // over the very model a window drives.
    ("zgui-testkit-view", "zgui-edit"),
    // The inspector is a view component, so the application that shows one mounts it, and an
    // application that mounts a component depends on the crate that defines it. The worked
    // examples are also the only place the inspector is exercised against a real window rather
    // than a harness — which is what it is for — so the alternative to this edge is a tool nothing
    // in the tree opens.
    ("zgui-examples", "zgui-devtools"),
];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    if tree.phases.is_empty() {
        report.skip("no docs/planning/PHASES.md to read the schedule from".to_owned());
        return report;
    }

    for member in &tree.members {
        if UNSCHEDULED.contains(&member.name.as_str()) {
            continue;
        }
        let key = if member.is_spike() {
            member.rel_dir.clone()
        } else {
            member.name.clone()
        };
        let Some(phase) = tree.phases.phase_of(&key) else {
            report.violation(
                member.manifest.rel_path.clone(),
                format!("`{key}` is not introduced by any phase in docs/planning/PHASES.md"),
            );
            continue;
        };

        for dependency in member.manifest.dependencies() {
            let Some(dependency_phase) = tree.phases.phase_of(&dependency.name) else {
                continue;
            };
            if dependency_phase > phase
                && !DRIVEN.contains(&(key.as_str(), dependency.name.as_str()))
            {
                report.violation(
                    member.manifest.rel_path.clone(),
                    format!(
                        "`{key}` arrives in phase {phase} but depends on `{}`, which arrives in phase {dependency_phase}",
                        dependency.name
                    ),
                );
            }
        }
    }
    report
}
