//! Workspace members: finding them, and the questions checks ask of one.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::ledger::tree::manifest::Manifest;
use crate::ledger::tree::sources::{self, SourceFile};

/// One workspace member.
#[derive(Debug, Clone)]
pub(crate) struct Member {
    /// The package name.
    pub(crate) name: String,
    /// The member directory, relative to the tree root.
    pub(crate) rel_dir: String,
    /// The member's manifest.
    pub(crate) manifest: Manifest,
    /// Every `.rs` and `.wgsl` file the member owns.
    pub(crate) sources: Vec<SourceFile>,
}

impl Member {
    /// Loads the member rooted at `dir`.
    fn load(root: &Path, dir: &Path) -> Result<Self> {
        let manifest = Manifest::load(root, &dir.join("Cargo.toml"))?;
        let name = manifest
            .package_name()
            .ok_or_else(|| {
                Error::failed(format!(
                    "{}: workspace member has no [package] name",
                    manifest.rel_path
                ))
            })?
            .to_owned();
        Ok(Self {
            name,
            rel_dir: super::relative(root, dir),
            sources: sources::collect(root, dir)?,
            manifest,
        })
    }

    /// The crate root source file, `src/lib.rs` or `src/main.rs`.
    pub(crate) fn crate_root(&self) -> Option<&SourceFile> {
        let lib = format!("{}/src/lib.rs", self.rel_dir);
        let main = format!("{}/src/main.rs", self.rel_dir);
        self.sources
            .iter()
            .find(|file| file.rel_path == lib || file.rel_path == main)
    }

    /// Whether the member is one of the risk-retirement spikes under `spikes/`.
    pub(crate) fn is_spike(&self) -> bool {
        self.rel_dir.starts_with("spikes/")
    }
}

/// Expands the root manifest's `members` globs into loaded members, honouring `exclude`.
pub(crate) fn discover(root: &Path, manifest: &Manifest) -> Result<Vec<Member>> {
    let exclude = manifest.workspace_list("exclude");
    let mut directories = Vec::new();
    for pattern in manifest.workspace_list("members") {
        expand(root, &pattern, &mut directories)?;
    }
    directories.sort();
    directories.dedup();

    let mut members = Vec::new();
    for directory in directories {
        let rel = super::relative(root, &directory);
        if exclude.iter().any(|excluded| rel.starts_with(excluded)) {
            continue;
        }
        if !directory.join("Cargo.toml").is_file() {
            continue;
        }
        members.push(Member::load(root, &directory)?);
    }
    members.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(members)
}

/// Expands one `members` entry, which is either a plain path or a path ending in `*`.
fn expand(root: &Path, pattern: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    let Some(prefix) = pattern.strip_suffix("/*") else {
        out.push(root.join(pattern));
        return Ok(());
    };
    let parent = root.join(prefix);
    if !parent.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(&parent).map_err(|source| Error::io(&parent, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::io(&parent, source))?;
        if entry.path().is_dir() {
            out.push(entry.path());
        }
    }
    Ok(())
}
