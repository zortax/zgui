//! Finding the style engine's property definitions on disk.
//!
//! The engine is an ordinary registry dependency, so its sources sit at a path derived from the
//! version the workspace's lock file resolved. Deriving it — rather than vendoring a copy — is the
//! whole point: a copy would go stale silently at the next upgrade, and a cross-check answered from
//! a stale copy is worse than none.

use std::path::{Path, PathBuf};

/// Where the definitions live, relative to the engine's own source directory.
const RELATIVE: &str = "properties/longhands.toml";

/// The environment variable that overrides the search, for a patched or vendored engine.
pub const OVERRIDE: &str = "ZGUI_STYLO_PROPERTIES";

/// Why the definitions could not be found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocateError {
    /// The lock file does not exist or does not resolve the engine.
    NoLockedVersion(String),
    /// The engine's sources were not where the version says they should be.
    NotUnpacked(PathBuf),
}

impl core::fmt::Display for LocateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoLockedVersion(reason) => {
                write!(f, "the style engine's locked version is unknown: {reason}")
            }
            Self::NotUnpacked(path) => write!(
                f,
                "no property definitions under {}; build the workspace once so the engine's \
                 sources are unpacked, or set {OVERRIDE}",
                path.display()
            ),
        }
    }
}

impl core::error::Error for LocateError {}

/// The path to the engine's property definitions.
///
/// # Errors
///
/// Returns [`LocateError`] rather than a default, because a cross-check that silently found nothing
/// to check against would pass while checking nothing — which is the failure the cross-check itself
/// exists to catch one level up.
pub fn source_path() -> Result<PathBuf, LocateError> {
    if let Some(path) = std::env::var_os(OVERRIDE) {
        return Ok(PathBuf::from(path));
    }
    let version = locked_version(&workspace_root().join("Cargo.lock"))?;
    let registry = home().join("registry/src");
    let entries =
        std::fs::read_dir(&registry).map_err(|_| LocateError::NotUnpacked(registry.clone()))?;
    for entry in entries.flatten() {
        let candidate = entry.path().join(format!("stylo-{version}")).join(RELATIVE);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(LocateError::NotUnpacked(registry))
}

/// The workspace root, found from this crate's own location.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("this crate lives two levels below the workspace root")
        .to_path_buf()
}

/// Where cargo keeps unpacked registry sources.
fn home() -> PathBuf {
    std::env::var_os("CARGO_HOME").map_or_else(
        || PathBuf::from(std::env::var_os("HOME").expect("a home directory")).join(".cargo"),
        PathBuf::from,
    )
}

/// The engine version the workspace's lock file resolved.
fn locked_version(lock: &Path) -> Result<String, LocateError> {
    let text = std::fs::read_to_string(lock)
        .map_err(|error| LocateError::NoLockedVersion(format!("{}: {error}", lock.display())))?;
    let document: toml::Table = text
        .parse()
        .map_err(|error| LocateError::NoLockedVersion(format!("{error}")))?;
    toml::Value::Table(document)
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .find(|package| package.get("name").and_then(toml::Value::as_str) == Some("stylo"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            LocateError::NoLockedVersion("no `stylo` package in the lock file".to_owned())
        })
}

#[cfg(test)]
mod tests {
    use super::source_path;

    /// The definitions are found, and they are the engine's own file rather than an empty one.
    ///
    /// The second half is the one that matters: a locator that answered a path to nothing would
    /// make every cross-check above it pass against an empty set of definitions.
    #[test]
    fn the_engines_definitions_are_found_and_are_not_empty() {
        let path = source_path().expect("the engine's sources are unpacked");
        let text = std::fs::read_to_string(&path).expect("readable");
        assert!(
            text.len() > 10_000,
            "{} is {} bytes",
            path.display(),
            text.len()
        );
        assert!(
            text.contains("[display]"),
            "{} does not define `display`",
            path.display()
        );
    }
}
