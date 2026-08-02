//! The versions ledger.
//!
//! Four separate silent failures live here: a wgpu that is not the one vello links, a style or
//! reactivity crate that drifted off its pinned patch release, a reactive graph built without
//! its effects (the whole UI compiles, runs and never updates), and a layout engine missing a
//! feature whose absence is not an error but a wrong answer.

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::manifest::Dependency;

/// Dependencies pinned to an exact patch release.
const PINNED: [(&str, &str); 4] = [
    ("stylo", "=0.19.0"),
    ("reactive_graph", "=0.2.14"),
    ("reactive_stores", "=0.4.3"),
    ("any_spawner", "=0.3.0"),
];

/// The major version every wgpu in the tree must have.
const WGPU_MAJOR: &str = "29.";

/// How many copies of a package the resolved graph may hold.
///
/// Two `skrifa` copies are expected: the text engine and the vector rasteriser are on different
/// releases. One `linebender_resource_handle` is what makes the two font-data types the same
/// type and the glyph bridge free.
const DUPLICATE_LIMITS: [(&str, usize); 3] = [
    ("wgpu", 1),
    ("skrifa", 2),
    ("linebender_resource_handle", 1),
];

/// The exact feature set the layout engine is built with.
///
/// The seven behavioural features are what the box tree relies on, and `taffy_tree` is
/// deliberately absent because we own the tree. `std` is here because taffy 0.12.2 does not
/// compile without it once `detailed_layout_info` and `grid` are on.
const TAFFY_FEATURES: [&str; 8] = [
    "block_layout",
    "calc",
    "content_size",
    "detailed_layout_info",
    "flexbox",
    "float_layout",
    "grid",
    "std",
];

/// Features of the reactive graph that must be on, and off.
const REACTIVE_ON: &str = "effects";
/// The feature our own nightly must not switch on.
const REACTIVE_OFF: &str = "nightly";

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let workspace = tree.manifest.workspace_dependencies();
    let at = tree.manifest.rel_path.clone();

    for (name, requirement) in PINNED {
        match find(&workspace, name) {
            None => report.skip(format!("`{name}` is not declared yet")),
            Some(dependency) => {
                if dependency.version.as_deref() != Some(requirement) {
                    report.violation(
                        at.clone(),
                        format!(
                            "`{name}` must be pinned `{requirement}`, found `{}`",
                            dependency.version.as_deref().unwrap_or("<unset>")
                        ),
                    );
                }
            }
        }
    }

    match find(&workspace, "wgpu") {
        None => report.skip("`wgpu` is not declared yet".to_owned()),
        Some(dependency) => {
            let version = dependency.version.as_deref().unwrap_or_default();
            if !version.starts_with(WGPU_MAJOR) {
                report.violation(
                    at.clone(),
                    format!("`wgpu` must be {WGPU_MAJOR}x, found `{version}`"),
                );
            }
        }
    }

    match find(&workspace, "taffy") {
        None => report.skip("`taffy` is not declared yet".to_owned()),
        Some(dependency) => {
            if dependency.default_features != Some(false) {
                report.violation(
                    at.clone(),
                    "`taffy` must set `default-features = false`: we own the tree".to_owned(),
                );
            }
            let mut declared = dependency.features.clone();
            declared.sort();
            if declared != TAFFY_FEATURES {
                report.violation(
                    at.clone(),
                    format!(
                        "`taffy` features must be exactly [{}], found [{}]",
                        TAFFY_FEATURES.join(", "),
                        declared.join(", ")
                    ),
                );
            }
        }
    }

    if let Some(dependency) = find(&workspace, "reactive_graph") {
        if !dependency.features.iter().any(|f| f == REACTIVE_ON) {
            report.violation(
                at.clone(),
                format!(
                    "`reactive_graph` must enable `{REACTIVE_ON}`, or the UI silently never updates"
                ),
            );
        }
        if dependency.features.iter().any(|f| f == REACTIVE_OFF) {
            report.violation(
                at.clone(),
                format!("`reactive_graph` must not enable `{REACTIVE_OFF}`"),
            );
        }
    }

    for (name, limit) in DUPLICATE_LIMITS {
        let versions = tree.lock.versions_of(name);
        if versions.is_empty() {
            report.skip(format!("`{name}` is not in the resolved graph yet"));
            continue;
        }
        if versions.len() > limit {
            report.violation(
                "Cargo.lock",
                format!(
                    "`{name}` resolves to {} versions ({}), at most {limit} permitted",
                    versions.len(),
                    versions.join(", ")
                ),
            );
        }
        if name == "wgpu" {
            for version in versions {
                if !version.starts_with(WGPU_MAJOR) {
                    report.violation(
                        "Cargo.lock",
                        format!("`wgpu` resolved to `{version}`, which is not {WGPU_MAJOR}x"),
                    );
                }
            }
        }
    }

    match tree.features.activates(REACTIVE_ON) {
        None => report.skip("the feature graph was not resolved".to_owned()),
        Some(true) => {}
        Some(false) => report.violation(
            "cargo tree --edges features",
            format!("`reactive_graph/{REACTIVE_ON}` is not activated in the resolved graph"),
        ),
    }
    if tree.features.activates(REACTIVE_OFF) == Some(true) {
        report.violation(
            "cargo tree --edges features",
            format!("`reactive_graph/{REACTIVE_OFF}` is activated in the resolved graph"),
        );
    }

    for member in &tree.members {
        for dependency in member.manifest.dependencies() {
            if dependency.name.starts_with("zgui") || dependency.path.is_some() {
                continue;
            }
            if !dependency.inherited {
                report.violation(
                    member.manifest.rel_path.clone(),
                    format!(
                        "`{}` must be inherited with `{}.workspace = true`, so its version lives in one place",
                        dependency.name, dependency.name
                    ),
                );
            }
        }
    }

    report
}

/// Finds one entry of a dependency table.
fn find<'a>(dependencies: &'a [Dependency], name: &str) -> Option<&'a Dependency> {
    dependencies
        .iter()
        .find(|dependency| dependency.name == name)
}
