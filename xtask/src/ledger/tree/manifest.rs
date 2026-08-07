//! Reading a `Cargo.toml` into the shape the ledger checks ask questions of.

use std::path::{Path, PathBuf};

use toml::{Table, Value};

use crate::error::{Error, Result, read_to_string};

/// What a manifest is called in the fixture trees under `xtask/fixtures/`.
///
/// The fixtures name their crates after real ones deliberately — the checks are keyed to those
/// names, so `unsafe` reads a fixture `zgui-arena` and `pinned` a fixture `zgui-view` — which
/// means their manifests cannot be called `Cargo.toml`. Cargo resolves a git dependency by
/// scanning the whole checkout for packages rather than by reading the workspace, and neither
/// `workspace.exclude` nor anything else suppresses that scan, so anyone depending on zgui by
/// git is told cargo is skipping a dozen duplicate `zgui-geom`s. Cargo matches the name
/// exactly, so a prefix hides the fixtures from it and from nothing else: the ledger finds its
/// manifests through [`path_in`], and the name still ends in `.toml` for every editor.
pub(crate) const FIXTURE_NAME: &str = "fixture.Cargo.toml";

/// The manifest of the package rooted at `dir`, under whichever of the two names it carries.
///
/// Returns the `Cargo.toml` path when the directory holds neither, so that a caller reporting a
/// missing manifest names the one a reader expects.
pub(crate) fn path_in(dir: &Path) -> PathBuf {
    let fixture = dir.join(FIXTURE_NAME);
    if fixture.is_file() {
        fixture
    } else {
        dir.join("Cargo.toml")
    }
}

/// Which dependency table an entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    /// `[dependencies]`.
    Normal,
    /// `[dev-dependencies]`.
    Dev,
    /// `[build-dependencies]`.
    Build,
}

impl Section {
    /// The table name as it is written in a manifest.
    pub(crate) fn table_name(self) -> &'static str {
        match self {
            Self::Normal => "dependencies",
            Self::Dev => "dev-dependencies",
            Self::Build => "build-dependencies",
        }
    }

    /// Every section, in manifest order.
    const ALL: [Self; 3] = [Self::Normal, Self::Dev, Self::Build];
}

/// One dependency entry, flattened out of whichever form it was written in.
#[derive(Debug, Clone)]
pub(crate) struct Dependency {
    /// The crate being depended on.
    ///
    /// This is the real crate name: an entry that renames its package with `package = "…"`
    /// reports the package, not the key it was written under.
    pub(crate) name: String,
    /// The key the entry was written under, which is what a `workspace = true` inheritance
    /// in a member manifest refers to.
    pub(crate) key: String,
    /// The table the entry came from.
    pub(crate) section: Section,
    /// The version requirement, if the entry states one directly.
    pub(crate) version: Option<String>,
    /// The features the entry turns on.
    pub(crate) features: Vec<String>,
    /// The `default-features` setting, if the entry states one.
    pub(crate) default_features: Option<bool>,
    /// Whether the entry is inherited with `workspace = true`.
    pub(crate) inherited: bool,
    /// The path, for a path dependency.
    pub(crate) path: Option<String>,
}

impl Dependency {
    /// Flattens one `name = <value>` entry.
    fn parse(name: &str, section: Section, value: &Value) -> Self {
        let mut dependency = Self {
            name: name.to_owned(),
            key: name.to_owned(),
            section,
            version: None,
            features: Vec::new(),
            default_features: None,
            inherited: false,
            path: None,
        };
        match value {
            Value::String(version) => dependency.version = Some(version.clone()),
            Value::Table(table) => {
                dependency.version = table
                    .get("version")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                dependency.path = table.get("path").and_then(Value::as_str).map(str::to_owned);
                dependency.inherited = table
                    .get("workspace")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                dependency.default_features =
                    table.get("default-features").and_then(Value::as_bool);
                if let Some(features) = table.get("features").and_then(Value::as_array) {
                    dependency.features = features
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                }
                // `package = "…"` renames the entry; the ledgers care about the real crate.
                if let Some(real) = table.get("package").and_then(Value::as_str) {
                    dependency.name = real.to_owned();
                }
            }
            _ => {}
        }
        dependency
    }
}

/// A parsed manifest, kept alongside the text it was parsed from.
#[derive(Debug, Clone)]
pub(crate) struct Manifest {
    /// The path relative to the tree root, for messages.
    pub(crate) rel_path: String,
    /// The raw text, for the checks that look for comment conventions.
    pub(crate) text: String,
    /// The parsed document.
    pub(crate) table: Table,
}

impl Manifest {
    /// Loads and parses the manifest at `path`.
    pub(crate) fn load(root: &Path, path: &Path) -> Result<Self> {
        let text = read_to_string(path)?;
        let table = text.parse::<Table>().map_err(|source| Error::Toml {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(Self {
            rel_path: super::relative(root, path),
            text,
            table,
        })
    }

    /// The `[package] name`, absent in a virtual manifest.
    pub(crate) fn package_name(&self) -> Option<&str> {
        self.table.get("package")?.as_table()?.get("name")?.as_str()
    }

    /// Whether the package is published to a registry.
    ///
    /// `publish = false` is the only way a member opts out, so anything else — including the
    /// absence of the key — is a package whose public surface reaches somebody outside this tree.
    pub(crate) fn is_published(&self) -> bool {
        let Some(package) = self.table.get("package").and_then(Value::as_table) else {
            return false;
        };
        !matches!(package.get("publish").and_then(Value::as_bool), Some(false))
    }

    /// Every dependency the manifest declares, including target-specific ones.
    pub(crate) fn dependencies(&self) -> Vec<Dependency> {
        let mut out = Vec::new();
        for section in Section::ALL {
            collect(&self.table, section, &mut out);
        }
        if let Some(targets) = self.table.get("target").and_then(Value::as_table) {
            for target in targets.values().filter_map(Value::as_table) {
                for section in Section::ALL {
                    collect(target, section, &mut out);
                }
            }
        }
        out
    }

    /// The `[workspace.dependencies]` table, empty for a member manifest.
    pub(crate) fn workspace_dependencies(&self) -> Vec<Dependency> {
        let Some(table) = self
            .table
            .get("workspace")
            .and_then(Value::as_table)
            .and_then(|workspace| workspace.get("dependencies"))
            .and_then(Value::as_table)
        else {
            return Vec::new();
        };
        table
            .iter()
            .map(|(name, value)| Dependency::parse(name, Section::Normal, value))
            .collect()
    }

    /// A table inside `[workspace]`, such as `package`, absent in a member manifest.
    pub(crate) fn workspace_table(&self, key: &str) -> Option<&Table> {
        self.table
            .get("workspace")
            .and_then(Value::as_table)
            .and_then(|workspace| workspace.get(key))
            .and_then(Value::as_table)
    }

    /// A `[workspace]` string list such as `members` or `exclude`.
    pub(crate) fn workspace_list(&self, key: &str) -> Vec<String> {
        self.table
            .get("workspace")
            .and_then(Value::as_table)
            .and_then(|workspace| workspace.get(key))
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Appends one dependency table's entries to `out`.
fn collect(table: &Table, section: Section, out: &mut Vec<Dependency>) {
    let Some(entries) = table.get(section.table_name()).and_then(Value::as_table) else {
        return;
    };
    out.extend(
        entries
            .iter()
            .map(|(name, value)| Dependency::parse(name, section, value)),
    );
}
