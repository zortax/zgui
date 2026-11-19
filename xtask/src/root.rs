//! Locating the workspace root without asking cargo.

use std::path::{Path, PathBuf};

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

/// Whether `directory` is cargo build output.
///
/// Cargo writes its output to a `target` directory next to a `Cargo.toml`. A `target` directory
/// in a source tree, such as `src/target`, is source and is read.
pub(crate) fn is_build_output(directory: &Path) -> bool {
    directory.file_name().is_some_and(|name| name == "target")
        && directory
            .parent()
            .is_some_and(|parent| parent.join("Cargo.toml").is_file())
}

#[cfg(test)]
mod tests {
    use super::is_build_output;

    #[test]
    fn only_a_target_next_to_a_manifest_is_build_output() {
        let root = std::env::temp_dir().join(format!("xtask-build-output-{}", std::process::id()));
        let source = root.join("src").join("target");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "").unwrap();

        assert!(is_build_output(&root.join("target")));
        assert!(!is_build_output(&source));
        assert!(!is_build_output(&root.join("src")));

        std::fs::remove_dir_all(&root).unwrap();
    }
}
