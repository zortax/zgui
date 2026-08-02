//! The resolved dependency versions, read straight out of `Cargo.lock`.

use std::path::Path;

use toml::{Table, Value};

use crate::error::{Error, Result, read_to_string};

/// One resolved package.
#[derive(Debug, Clone)]
pub(crate) struct LockedPackage {
    /// The package name.
    pub(crate) name: String,
    /// The exact version the resolver chose.
    pub(crate) version: String,
}

/// Every package in the lockfile.
#[derive(Debug, Clone, Default)]
pub(crate) struct Lock {
    /// The resolved packages, in lockfile order.
    pub(crate) packages: Vec<LockedPackage>,
}

impl Lock {
    /// Reads `<root>/Cargo.lock`, returning an empty lock when there is none.
    pub(crate) fn load(root: &Path) -> Result<Self> {
        let path = root.join("Cargo.lock");
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = read_to_string(&path)?;
        let table = text.parse::<Table>().map_err(|source| Error::Toml {
            path: path.clone(),
            source,
        })?;
        let packages = table
            .get("package")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Value::as_table)
                    .filter_map(|entry| {
                        Some(LockedPackage {
                            name: entry.get("name")?.as_str()?.to_owned(),
                            version: entry.get("version")?.as_str()?.to_owned(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self { packages })
    }

    /// Every resolved version of `name`.
    pub(crate) fn versions_of(&self, name: &str) -> Vec<&str> {
        self.packages
            .iter()
            .filter(|package| package.name == name)
            .map(|package| package.version.as_str())
            .collect()
    }
}
