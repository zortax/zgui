//! Locating the workspace root without asking cargo.

use std::path::PathBuf;

use crate::error::{Error, Result};

/// Returns the workspace root directory.
///
/// xtask lives at `<root>/xtask`, and cargo sets `CARGO_MANIFEST_DIR` for the running binary,
/// so the root is one level up. Falling back to the current directory keeps the tool usable
/// when it is invoked as a plain binary.
pub(crate) fn workspace_root() -> Result<PathBuf> {
    if let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") {
        let candidate = PathBuf::from(manifest_dir);
        if let Some(parent) = candidate.parent()
            && parent.join("Cargo.toml").is_file()
        {
            return Ok(parent.to_path_buf());
        }
    }
    let current = std::env::current_dir().map_err(|source| Error::io(".", source))?;
    for directory in current.ancestors() {
        if directory.join("Cargo.toml").is_file() && directory.join("xtask").is_dir() {
            return Ok(directory.to_path_buf());
        }
    }
    Err(Error::failed(
        "could not find the workspace root: no ancestor directory holds both Cargo.toml and xtask/",
    ))
}
