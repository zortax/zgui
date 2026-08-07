//! A snapshot of one workspace tree, gathered once and shared by every ledger check.
//!
//! The checks are pure functions of a [`Tree`], which is what lets each of them run against a
//! planted-violation fixture that is an ordinary little workspace on disk rather than against
//! a mock.

pub(crate) mod features;
pub(crate) mod lock;
pub(crate) mod manifest;
pub(crate) mod member;
pub(crate) mod sources;

use std::path::Path;

use crate::error::Result;
use crate::ledger::phases::PhaseMap;
use crate::ledger::tree::features::{FeatureSource, FeatureTree};
use crate::ledger::tree::lock::Lock;
use crate::ledger::tree::manifest::Manifest;
use crate::ledger::tree::member::Member;

/// Everything the ledger checks read.
#[derive(Debug)]
pub(crate) struct Tree {
    /// The root manifest.
    pub(crate) manifest: Manifest,
    /// Every workspace member.
    pub(crate) members: Vec<Member>,
    /// Every markdown file that describes the tree as it is.
    pub(crate) prose: Vec<sources::SourceFile>,
    /// The `NOTICE` text, empty when the tree has none.
    pub(crate) notice: String,
    /// The resolved versions.
    pub(crate) lock: Lock,
    /// Which phase introduces which crate.
    pub(crate) phases: PhaseMap,
    /// The resolved feature graph.
    pub(crate) features: FeatureTree,
}

impl Tree {
    /// Gathers the tree rooted at `root`.
    pub(crate) fn gather(root: &Path, features: FeatureSource) -> Result<Self> {
        let manifest = Manifest::load(root, &manifest::path_in(root))?;
        let members = member::discover(root, &manifest)?;
        let notice_path = root.join("NOTICE");
        let notice = if notice_path.is_file() {
            crate::error::read_to_string(&notice_path)?
        } else {
            String::new()
        };
        Ok(Self {
            manifest,
            members,
            prose: sources::prose(root)?,
            notice,
            lock: Lock::load(root)?,
            phases: PhaseMap::load(root)?,
            features: FeatureTree::gather(root, features)?,
        })
    }
}

/// Renders `path` relative to `root` with forward slashes, for stable messages.
pub(crate) fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
