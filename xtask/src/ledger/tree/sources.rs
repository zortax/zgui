//! Walking a member's source files.

use std::path::Path;

use crate::error::{Error, Result, read_to_string};

/// One source file, with its text held in memory.
#[derive(Debug, Clone)]
pub(crate) struct SourceFile {
    /// The path relative to the tree root, for messages and for `NOTICE` lookups.
    pub(crate) rel_path: String,
    /// The file's contents.
    pub(crate) text: String,
}

/// The file extensions the ledgers read.
const EXTENSIONS: [&str; 2] = ["rs", "wgsl"];

/// Directory names never descended into.
const SKIP: [&str; 3] = ["target", ".git", "fixtures"];

/// Directories holding a record of what was once proposed, which is not a document to keep up to
/// date and is therefore not read by the checks that read prose.
///
/// `planning` covers both the live plans and `planning/outdated`: a plan states what a tree will
/// be, not what it is, and a superseded one states what a tree was going to be. Rewriting either to
/// satisfy a check that reads prose would falsify it.
const RECORDS: [&str; 2] = ["planning", "research"];

/// Reads every source file under `directory`, depth first, skipping build output.
pub(crate) fn collect(root: &Path, directory: &Path) -> Result<Vec<SourceFile>> {
    let mut out = Vec::new();
    walk(root, directory, &mut out)?;
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(out)
}

/// Reads every markdown file under `root` that describes the tree as it is.
///
/// A record of what was proposed at a time is left out: rewriting one would falsify it, so a check
/// that read it would be asking a historical document to describe a tree it predates.
pub(crate) fn prose(root: &Path) -> Result<Vec<SourceFile>> {
    let mut out = Vec::new();
    markdown(root, root, &mut out)?;
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(out)
}

/// The recursive half of [`prose`].
fn markdown(root: &Path, directory: &Path, out: &mut Vec<SourceFile>) -> Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(directory).map_err(|source| Error::io(directory, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::io(directory, source))?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if SKIP.contains(&name.as_ref()) || RECORDS.contains(&name.as_ref()) {
                continue;
            }
            markdown(root, &path, out)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
            out.push(SourceFile {
                rel_path: super::relative(root, &path),
                text: read_to_string(&path)?,
            });
        }
    }
    Ok(())
}

/// The recursive half of [`collect`].
fn walk(root: &Path, directory: &Path, out: &mut Vec<SourceFile>) -> Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(directory).map_err(|source| Error::io(directory, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::io(directory, source))?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if SKIP.contains(&name.as_ref()) {
                continue;
            }
            walk(root, &path, out)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| EXTENSIONS.contains(&extension))
        {
            out.push(SourceFile {
                rel_path: super::relative(root, &path),
                text: read_to_string(&path)?,
            });
        }
    }
    Ok(())
}
