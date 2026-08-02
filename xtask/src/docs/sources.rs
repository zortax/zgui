//! Finding the files a documentation gate reads.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Every `.rs` file under `directory`, in a stable order.
///
/// Sorted so that a failing run names its violations in the same order on every machine, which is
/// what makes the output of two runs comparable.
pub(crate) fn rust_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    collect(directory, "rs", &mut found)?;
    found.sort();
    Ok(found)
}

/// Every `.md` file directly inside `directory`, in a stable order.
pub(crate) fn markdown_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if !directory.is_dir() {
        return Ok(found);
    }
    for entry in std::fs::read_dir(directory).map_err(|source| Error::io(directory, source))? {
        let entry = entry.map_err(|source| Error::io(directory, source))?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "md") {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// Walks `directory`, collecting files whose extension is `extension`.
fn collect(directory: &Path, extension: &str, found: &mut Vec<PathBuf>) -> Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(directory).map_err(|source| Error::io(directory, source))? {
        let entry = entry.map_err(|source| Error::io(directory, source))?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect(&path, extension, found)?;
        } else if path
            .extension()
            .is_some_and(|found_extension| found_extension == extension)
        {
            found.push(path);
        }
    }
    Ok(())
}

/// `path` relative to the workspace root, for a message a reader can paste into an editor.
pub(crate) fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
