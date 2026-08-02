//! The resolved feature graph, which is the one ledger input cargo has to compute.

use std::path::Path;

use crate::error::{Result, read_to_string};
use crate::process;

/// Where the feature graph comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FeatureSource {
    /// Ask cargo to resolve it.
    Cargo,
    /// Read it from `tree-features.txt` beside the tree root, as the fixtures do.
    ///
    /// A root with no such file leaves the graph unresolved, and the checks that need it
    /// report themselves skipped rather than guessing.
    Recorded,
}

/// The package whose features the versions ledger asserts on.
const SUBJECT: &str = "reactive_graph";

/// A `cargo tree -e features` rendering, or nothing if it could not be obtained.
#[derive(Debug, Clone, Default)]
pub(crate) struct FeatureTree {
    /// The rendered tree, absent when the subject package is not in the graph at all.
    pub(crate) text: Option<String>,
}

impl FeatureTree {
    /// Obtains the feature graph from `source`.
    pub(crate) fn gather(root: &Path, source: FeatureSource) -> Result<Self> {
        let text = match source {
            FeatureSource::Recorded => {
                let path = root.join("tree-features.txt");
                if path.is_file() {
                    Some(read_to_string(&path)?)
                } else {
                    None
                }
            }
            FeatureSource::Cargo => {
                let cargo = process::cargo();
                process::capture(
                    root,
                    &cargo,
                    &[
                        "tree",
                        "--workspace",
                        "--edges",
                        "features",
                        "--invert",
                        SUBJECT,
                    ],
                )
                .ok()
            }
        };
        Ok(Self { text })
    }

    /// Whether the rendering activates `<subject>/<feature>`.
    pub(crate) fn activates(&self, feature: &str) -> Option<bool> {
        let text = self.text.as_deref()?;
        let needle = format!("{SUBJECT} feature \"{feature}\"");
        Some(text.contains(&needle))
    }
}
