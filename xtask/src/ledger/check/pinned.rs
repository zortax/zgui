//! The two dependency lists that are pinned exactly, and the edge that must not exist.
//!
//! Most crates in this tree may grow a dependency without anyone thinking hard about it. Two may
//! not, and one edge may never be drawn at all:
//!
//! * **`zgui-view`'s dependency list is exactly four crates.** The view layer is what a future
//!   backend — a browser's own nodes, a recorder, something not yet thought of — is substituted
//!   underneath, and it can only be substituted underneath something that cannot see a document,
//!   a style engine, a layout engine, a renderer or a window system. One added dependency and
//!   that stops being true, silently, and nothing else in the tree notices.
//! * **`zgui-vocab`'s is exactly three.** It exists so that the view layer and the document can
//!   name the same types without either depending on the other; a fourth dependency would make it
//!   a bridge that carries something.
//! * **No crate below the view layer may name it.** Event dispatch is the temptation: resolving
//!   which listeners an event reaches is a job for the layer that owns hit testing, and calling
//!   one is a job for the layer above. A crate that named the view layer in order to call a
//!   handler directly would invert the whole graph.

use std::collections::BTreeSet;

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::manifest::Section;

/// The crates whose dependency list is pinned exactly, and to what.
const PINNED: &[(&str, &[&str])] = &[
    (
        "zgui-view",
        &["zgui-geom", "zgui-interned", "zgui-reactive", "zgui-vocab"],
    ),
    ("zgui-vocab", &["accesskit", "zgui-geom", "zgui-interned"]),
];

/// What a pinned crate may not name in *any* section, development included.
///
/// The exact list above already forbids these as ordinary dependencies. They are named again
/// because a development dependency is still a thing the crate can see, and a test that reached
/// for a document or a window would be evidence that the seam had stopped being enough.
const NEVER_VISIBLE: &[&str] = &[
    "stylo",
    "stylo_dom",
    "selectors",
    "taffy",
    "parley",
    "wgpu",
    "vello",
    "winit",
    "zgui-dom",
    "zgui-layout",
    "zgui-style",
    "zgui-render",
    "zgui-scene",
    "zgui-platform",
];

/// The crates that sit below the view layer and therefore may not name it.
///
/// Each is driven by the runtime above it and talks about the view layer's vocabulary, which is
/// exactly what makes the upward edge tempting — and each has a way to do its job without one.
const BELOW_THE_VIEW_LAYER: &[&str] = &[
    "zgui-input",
    "zgui-scroll",
    "zgui-a11y",
    "zgui-anim",
    "zgui-edit",
    "zgui-layout",
    "zgui-style",
    "zgui-dom",
    "zgui-text",
    "zgui-paint",
    "zgui-scene",
    "zgui-render",
    "zgui-platform",
];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let mut checked = BTreeSet::new();

    for member in &tree.members {
        if let Some((_, expected)) = PINNED.iter().find(|(name, _)| *name == member.name) {
            checked.insert(member.name.clone());
            let declared = member.manifest.dependencies();
            let found: BTreeSet<&str> = declared
                .iter()
                .filter(|dependency| dependency.section == Section::Normal)
                .map(|dependency| dependency.name.as_str())
                .collect();
            let expected: BTreeSet<&str> = expected.iter().copied().collect();

            for extra in found.difference(&expected) {
                report.violation(
                    member.manifest.rel_path.clone(),
                    format!(
                        "`{}` depends on `{extra}`, and its list is pinned to exactly [{}]",
                        member.name,
                        expected.iter().copied().collect::<Vec<_>>().join(", ")
                    ),
                );
            }
            for forbidden in declared
                .iter()
                .filter(|dependency| NEVER_VISIBLE.contains(&dependency.name.as_str()))
            {
                report.violation(
                    member.manifest.rel_path.clone(),
                    format!(
                        "`{}` names `{}` in [{}]; it must not be able to see one at all",
                        member.name,
                        forbidden.name,
                        forbidden.section.table_name()
                    ),
                );
            }

            for missing in expected.difference(&found) {
                report.violation(
                    member.manifest.rel_path.clone(),
                    format!(
                        "`{}` no longer depends on `{missing}`, which its pinned list names",
                        member.name
                    ),
                );
            }
        }

        let declared = member.manifest.dependencies();
        if BELOW_THE_VIEW_LAYER.contains(&member.name.as_str())
            && let Some(edge) = declared
                .iter()
                .find(|dependency| dependency.name == "zgui-view")
        {
            report.violation(
                member.manifest.rel_path.clone(),
                format!(
                    "`{}` names `zgui-view` in [{}]; nothing below the view layer may, because \
                     the layer above it is what calls into it",
                    member.name,
                    edge.section.table_name()
                ),
            );
        }
    }

    for (name, _) in PINNED {
        if !checked.contains(*name) {
            report.skip(format!("`{name}` is not in this tree yet"));
        }
    }
    report
}
