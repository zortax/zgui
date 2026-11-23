//! The compositor ledger.
//!
//! zgui draws through the shared compositor's interface, `compositor-types` and `compositor-api`,
//! and holds every implementation as a `Box<dyn CompositorClient>`, `Box<dyn Compositor>`,
//! `Box<dyn Renderer>` or `Box<dyn Presenter>`. The implementations are chosen in a few crates only: the umbrella's app
//! wiring, the crates that are wgpu by nature, the bench binaries and the test kit's factory. Any
//! other crate that names an implementation, or downcasts to one of its renderers or presenters,
//! could come to rely on behaviour the contract does not promise, and would then break with a
//! second implementation that is correct.
//!
//! Every crate of the compositor's checkout other than the two contract crates is an
//! implementation. The checkout is the directory that holds `compositor-types` at `crates/types`.
//!
//! A member is checked in three ways:
//!
//! 1. No manifest section depends on an implementation crate.
//! 2. No line of code names an implementation crate's path.
//! 3. No downcast targets a concrete renderer, presenter or compositor type.

mod code;

use std::collections::BTreeMap;
use std::path::Path;

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::manifest::Dependency;
use crate::ledger::tree::member::Member;

/// The compositor crates that state the contract.
const CONTRACT: [&str; 2] = ["compositor-types", "compositor-api"];

/// Where `compositor-types` sits in the compositor's checkout.
const TYPES_IN_CHECKOUT: &str = "crates/types";

/// The members that may name an implementation, and why.
const ALLOWED: &[(&str, &str)] = &[
    ("zgui", "the app wiring chooses the implementations"),
    ("zgui-wgpu", "embeds wgpu surfaces, which is wgpu by nature"),
    ("zgui-console", "presents to a display it drives itself"),
    (
        "zgui-platform-drm",
        "composes into buffers a display scans out",
    ),
    ("zgui-bench", "measures one implementation on one device"),
    (
        "zgui-testkit-compositor",
        "holds the one factory every test takes its compositor from",
    ),
];

/// The members outside [`ALLOWED`] that may name one implementation, the implementation, and why.
const EXCEPTIONS: &[(&str, &str, &str)] = &[(
    "zgui-shader-macro",
    "compositor-wgsl",
    "compiles an application shader into the reference renderer's prebuilt IR, which only that \
     renderer's assembly produces",
)];

/// Members outside every ledger.
const EXEMPT: [&str; 2] = ["probe", "xtask"];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let workspace = tree.manifest.workspace_dependencies();
    let aliases = aliases(&workspace);
    let Some(implementations) = implementations(&workspace) else {
        report.violation(
            tree.manifest.rel_path.clone(),
            format!(
                "no `compositor-types` path dependency ends in `{TYPES_IN_CHECKOUT}`, so the \
                 compositor's crates cannot be told apart"
            ),
        );
        return report;
    };

    for member in tree.members.iter().filter(|member| !is_allowed(member)) {
        let named: Vec<&str> = implementations
            .iter()
            .map(String::as_str)
            .filter(|name| !is_excepted(&member.name, name))
            .collect();
        for dependency in member.manifest.dependencies() {
            let name = resolve(&dependency, &aliases);
            if named.contains(&name.as_str()) {
                report.violation(
                    member.manifest.rel_path.clone(),
                    format!(
                        "`{}` depends on `{name}` in [{}]; only {} may name a compositor \
                         implementation",
                        member.name,
                        dependency.section.table_name(),
                        allowed_names()
                    ),
                );
            }
        }
        for source in &member.sources {
            for (line, finding) in code::findings(&source.text, &named) {
                report.violation(
                    source.rel_path.clone(),
                    format!("line {line}: {finding}; only {} may", allowed_names()),
                );
            }
        }
    }
    report
}

/// Returns the packages of the compositor's checkout outside [`CONTRACT`], or `None` when the
/// workspace gives `compositor-types` no path inside a checkout.
fn implementations(workspace: &[Dependency]) -> Option<Vec<String>> {
    let types = workspace
        .iter()
        .find(|dependency| dependency.name == "compositor-types")?;
    let types = Path::new(types.path.as_deref()?);
    if !types.ends_with(TYPES_IN_CHECKOUT) {
        return None;
    }
    let checkout = types.ancestors().nth(2)?;

    Some(
        workspace
            .iter()
            .filter(|dependency| {
                dependency
                    .path
                    .as_deref()
                    .is_some_and(|path| Path::new(path).starts_with(checkout))
            })
            .map(|dependency| dependency.name.clone())
            .filter(|name| !CONTRACT.contains(&name.as_str()))
            .collect(),
    )
}

/// Whether [`EXCEPTIONS`] lets `member` name the implementation `name`.
fn is_excepted(member: &str, name: &str) -> bool {
    EXCEPTIONS
        .iter()
        .any(|(excepted, implementation, _)| *excepted == member && *implementation == name)
}

/// Whether `member` may name implementations, or sits outside the ledgers.
fn is_allowed(member: &Member) -> bool {
    EXEMPT.contains(&member.name.as_str())
        || member.is_spike()
        || ALLOWED.iter().any(|(name, _)| *name == member.name)
}

/// The allowed members, for messages.
fn allowed_names() -> String {
    ALLOWED
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The `[workspace.dependencies]` keys that stand for a differently named package.
fn aliases(workspace: &[Dependency]) -> BTreeMap<&str, &str> {
    workspace
        .iter()
        .filter(|dependency| dependency.key != dependency.name)
        .map(|dependency| (dependency.key.as_str(), dependency.name.as_str()))
        .collect()
}

/// The package an entry stands for, through a workspace alias when it inherits one.
fn resolve(dependency: &Dependency, aliases: &BTreeMap<&str, &str>) -> String {
    match aliases.get(dependency.key.as_str()) {
        Some(real) if dependency.inherited => (*real).to_owned(),
        _ => dependency.name.clone(),
    }
}
