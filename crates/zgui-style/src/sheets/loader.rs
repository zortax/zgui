//! Where the source of a referenced stylesheet comes from.
//!
//! Two implementations ship, and they are the two an application actually has. One holds the
//! sheets that were compiled into the binary, which is what a component library's own styles are.
//! The other reads them from a directory, which is what makes editing a `.css` file and seeing the
//! running application change possible at all.
//!
//! Neither resolves a URL: both treat a reference as a name relative to what they hold, because a
//! document core that took a position on what `../theme.css` means would have to be overridden
//! rather than implemented.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use zgui_dom::{SheetLoader, SheetRequest};
use zgui_vocab::SharedString;

/// Stylesheets compiled into the binary, looked up by name.
///
/// A reference to a name this does not hold is refused rather than reported as pending, because
/// nothing will ever arrive to satisfy it and a silent nothing is worse than a parse error naming
/// the line.
#[derive(Clone, Debug, Default)]
pub struct EmbeddedSheets {
    /// Name to source.
    sheets: BTreeMap<String, SharedString>,
}

impl EmbeddedSheets {
    /// A loader holding nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one sheet under `name`, replacing any sheet already held under it.
    #[must_use]
    pub fn with(mut self, name: &str, source: &str) -> Self {
        self.sheets
            .insert(name.to_owned(), SharedString::from(source));
        self
    }

    /// How many sheets are held.
    pub fn len(&self) -> usize {
        self.sheets.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.sheets.is_empty()
    }
}

impl SheetLoader for EmbeddedSheets {
    /// Matches `href` against the names held, then against its final path segment.
    ///
    /// The second attempt is what makes an `@import` work: the parser resolves the reference
    /// against the sheet's base before handing it over, so a sheet embedded under `theme.css` is
    /// asked for as `zgui:///theme.css`. Nothing here parses a URL to discover that — the name is
    /// whatever follows the last separator, which is all a set of embedded names can mean.
    fn load(&self, _base: &str, href: &str) -> SheetRequest {
        let name = href.rsplit('/').next().unwrap_or(href);
        match self.sheets.get(href).or_else(|| self.sheets.get(name)) {
            Some(source) => SheetRequest::Ready(source.clone()),
            None => SheetRequest::Rejected,
        }
    }
}

/// Stylesheets read from one directory.
///
/// A reference that would leave the directory — an absolute path, or one climbing out through
/// `..` — is refused. That is not a security boundary and does not claim to be one; it is the rule
/// that keeps a sheet's references meaning what they say relative to where the sheets are.
#[derive(Clone, Debug)]
pub struct FilesystemSheets {
    /// The directory references are resolved inside.
    root: PathBuf,
}

impl FilesystemSheets {
    /// A loader reading from `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory references are resolved inside.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `href` as a path inside the root, or nothing when it would leave it.
    fn resolve(&self, href: &str) -> Option<PathBuf> {
        let relative = Path::new(href);
        let mut resolved = self.root.clone();
        for component in relative.components() {
            match component {
                Component::Normal(part) => resolved.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            }
        }
        Some(resolved)
    }
}

impl SheetLoader for FilesystemSheets {
    /// Reads `href` from inside the root, taking its final path segment when it arrives resolved.
    fn load(&self, _base: &str, href: &str) -> SheetRequest {
        let href = href.strip_prefix("zgui:///").unwrap_or(href);
        let Some(path) = self.resolve(href) else {
            return SheetRequest::Rejected;
        };
        match std::fs::read_to_string(&path) {
            Ok(source) => SheetRequest::Ready(SharedString::from(source.as_str())),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "a referenced stylesheet could not be read");
                SheetRequest::Rejected
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::{SheetLoader, SheetRequest};

    use super::{EmbeddedSheets, FilesystemSheets};

    #[test]
    fn an_embedded_sheet_is_returned_by_name_and_an_unknown_one_is_refused() {
        let loader = EmbeddedSheets::new().with("theme.css", "root { color: rgb(1, 2, 3) }");
        assert_eq!(loader.len(), 1);
        match loader.load("zgui:///", "theme.css") {
            SheetRequest::Ready(source) => assert!(source.as_ref().contains("color")),
            other => panic!("the sheet is held, so it is ready: {other:?}"),
        }
        assert!(matches!(
            loader.load("zgui:///", "missing.css"),
            SheetRequest::Rejected
        ));
    }

    #[test]
    fn a_filesystem_reference_that_would_leave_the_directory_is_refused() {
        let loader = FilesystemSheets::new("/does/not/exist");
        for href in ["../outside.css", "/etc/passwd"] {
            assert!(
                matches!(loader.load("zgui:///", href), SheetRequest::Rejected),
                "`{href}` resolves outside the root"
            );
        }
    }
}
