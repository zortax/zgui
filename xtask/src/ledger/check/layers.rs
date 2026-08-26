//! The layer ledger.
//!
//! The workspace's one architectural rule: a dependency points at the crate's own layer or at a
//! lower one. `docs/guide/layering.md` states it, and every layered manifest declares its layer
//! in a `# L<n> — …` header.
//!
//! [`topo`](super::topo) asks a related question — a crate may not depend on one that arrives in
//! a later implementation phase — against the schedule in `docs/planning/PHASES.md`, a local file
//! that is never committed. On a clone that has none, `topo` prints `ok` and a second line saying
//! it skipped. This check reads only what is committed, so a fork with no schedule gets the same
//! answer as the tree the schedule was written for.
//!
//! # Members with no header
//!
//! A member with no layer is a member whose edges nothing compares. Leaving one unreported would
//! make the check quietest about the crates it can say least about, and a crate added tomorrow
//! would leave the graph through the same gap. Every member is therefore either checked, named in
//! [`UNLAYERED`], or a spike, and a new crate that ships without its header fails the way a new
//! crate that points upward does.
//!
//! # Which sections count
//!
//! `[dependencies]` and `[build-dependencies]`. Both reach whoever builds this framework, so both
//! carry the shape the guide describes.
//!
//! `[dev-dependencies]` are left out, on the evidence of the tree. Fifteen development edges point
//! upward today and every one is the same shape: a crate drives its own subject through the stack
//! above it. Six crates between L2 and L6 take `zgui-testkit-scene` at L7 for the golden-image
//! comparison they are checked with, `zgui-render-vector-vello` at L2 takes the L3 and L4 engines
//! whose output it rasterises, and `zgui-platform-winit` at L2 takes `zgui-runtime` at L7 to drive
//! a real window round a real frame loop. A test binary links none of that into a consumer's
//! build, so those edges leave the shipped graph as layered as it reads. Where a development edge
//! does have to be forbidden the seam itself is the point, and [`pinned`](super::pinned) names it
//! there: `zgui-view` and `zgui-vocab` are pinned across every section, development dependencies
//! included.

use std::collections::BTreeMap;

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::manifest::Section;
use crate::ledger::tree::member::Member;

/// Members outside the layered graph, and why.
///
/// `probe` is the compile canary for the pinned external engines, and the guide states outright
/// that it is in no layer. `xtask` is this gate runner, which builds before the graph exists.
/// [`is_unlayered`] adds the spikes, because a spike retires into a crate rather than becoming one.
const UNLAYERED: [&str; 2] = ["probe", "xtask"];

/// The highest layer in the graph.
///
/// `docs/guide/layering.md` names nine, L0 to L8. A header above this is a typo that would make a
/// crate permissive about every edge it draws, so it is reported rather than believed.
const HIGHEST: u32 = 8;

/// What follows the number in a layer header: a space, then an em dash.
const SEPARATOR: &str = " —";

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let layers: BTreeMap<&str, u32> = tree
        .members
        .iter()
        .filter_map(|member| Some((member.name.as_str(), layer_of(&member.manifest.text)?)))
        .collect();

    for member in &tree.members {
        if is_unlayered(member) {
            continue;
        }
        let Some(&layer) = layers.get(member.name.as_str()) else {
            report.violation(
                member.manifest.rel_path.clone(),
                format!(
                    "`{}` declares no layer; a layered manifest carries a `# L<n>{SEPARATOR} …` \
                     header above its dependencies",
                    member.name
                ),
            );
            continue;
        };
        if layer > HIGHEST {
            report.violation(
                member.manifest.rel_path.clone(),
                format!(
                    "`{}` declares L{layer}, and the graph has nine layers, L0 to L{HIGHEST}",
                    member.name
                ),
            );
            continue;
        }

        for dependency in member.manifest.dependencies() {
            if dependency.section == Section::Dev {
                continue;
            }
            let Some(&dependency_layer) = layers.get(dependency.name.as_str()) else {
                continue;
            };
            if dependency_layer > layer {
                report.violation(
                    member.manifest.rel_path.clone(),
                    format!(
                        "`{}` is L{layer} and names `{}`, which is L{dependency_layer}",
                        member.name, dependency.name
                    ),
                );
            }
        }
    }
    report
}

/// Whether a member sits outside the layered graph.
fn is_unlayered(member: &Member) -> bool {
    UNLAYERED.contains(&member.name.as_str()) || member.is_spike()
}

/// The layer a manifest declares.
///
/// The header sits above the dependency tables. It is line 7 in most manifests and further down in
/// the ones that carry a `publish` key or a commented licence, so every line is offered to the
/// parser.
fn layer_of(manifest: &str) -> Option<u32> {
    manifest.lines().find_map(header)
}

/// Reads one line as a layer header.
fn header(line: &str) -> Option<u32> {
    let rest = line.strip_prefix("# L")?;
    let end = rest.find(|character: char| !character.is_ascii_digit())?;
    let (digits, tail) = rest.split_at(end);
    tail.starts_with(SEPARATOR).then(|| digits.parse().ok())?
}

#[cfg(test)]
mod tests {
    use super::layer_of;

    #[test]
    fn reads_a_well_formed_header() {
        let manifest = "[package]\nname = \"zgui-drm\"\n\n\
             # L2 — backends. Every dependency is inherited: `foo.workspace = true`.\n\
             [dependencies]\n";
        assert_eq!(layer_of(manifest), Some(2));
    }

    #[test]
    fn reads_no_layer_from_a_manifest_that_declares_none() {
        let manifest = "[package]\nname = \"probe\"\n\n\
             # probe is the stack-compiles canary: it names every engine at once.\n\
             [dependencies]\n";
        assert_eq!(layer_of(manifest), None);
    }

    #[test]
    fn reads_no_layer_from_a_header_written_the_wrong_way() {
        // A hyphen where the header takes an em dash.
        assert_eq!(layer_of("# L2 - backends\n"), None);
        // The layer spelt out.
        assert_eq!(layer_of("# Layer 2 — backends\n"), None);
        // A number with nothing after it.
        assert_eq!(layer_of("# L2\n"), None);
    }
}
