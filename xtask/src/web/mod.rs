//! Building an example for a browser, and serving it.
//!
//! A browser build has three steps that cargo does not do: compile for `wasm32`, generate the
//! JavaScript bindings with `wasm-bindgen`, and write the page that loads both. `cargo xtask web
//! build` does the three, and `cargo xtask web serve` then serves the result on this machine.
//!
//! The output is `target/web/<example>/`:
//!
//! | Path | Content |
//! |---|---|
//! | `index.html` | the host page |
//! | `pkg/<example>.js` | the bindings and the loader |
//! | `pkg/<example>_bg.wasm` | the application |
//! | `fonts/` | the font the page preloads |

mod page;
mod serve;

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::process;

/// The compilation target of a browser build.
const TARGET: &str = "wasm32-unknown-unknown";

/// The font the host page preloads: a browser gives a page no installed faces.
const FONT: &str = "crates/zgui-text-parley/tests/fonts/NotoSans-Regular.ttf";

/// What to build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Build {
    /// The example's name.
    pub(crate) example: String,
    /// The package that holds the example.
    pub(crate) package: String,
    /// Whether to build the unoptimised profile.
    pub(crate) dev: bool,
}

impl Build {
    /// The cargo profile and the directory cargo writes it to.
    const fn profile(&self) -> (&'static str, &'static str) {
        if self.dev {
            ("dev", "debug")
        } else {
            ("web", "web")
        }
    }
}

/// Builds `build` and returns the directory that holds the page.
pub(crate) fn build(root: &Path, build: &Build) -> Result<PathBuf> {
    check_bindgen(root)?;
    let (profile, directory) = build.profile();
    println!("building {} for {TARGET} ({profile})", build.example);
    process::run(
        root,
        &process::cargo(),
        &[
            "build",
            "--target",
            TARGET,
            "--profile",
            profile,
            "--package",
            &build.package,
            "--example",
            &build.example,
        ],
        &[],
    )?;

    let out = root.join("target/web").join(&build.example);
    let module = root
        .join("target")
        .join(TARGET)
        .join(directory)
        .join("examples")
        .join(format!("{}.wasm", build.example));
    let pkg = out.join("pkg");
    fs::create_dir_all(&pkg).map_err(|source| Error::io(&pkg, source))?;
    process::run(
        root,
        "wasm-bindgen",
        &[
            &module.to_string_lossy(),
            "--target",
            "web",
            "--no-typescript",
            "--out-dir",
            &pkg.to_string_lossy(),
            "--out-name",
            &build.example,
        ],
        &[],
    )?;

    let fonts = out.join("fonts");
    fs::create_dir_all(&fonts).map_err(|source| Error::io(&fonts, source))?;
    let font = root.join(FONT);
    let font_name = font
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    fs::copy(&font, fonts.join(&font_name)).map_err(|source| Error::io(&font, source))?;

    let index = out.join("index.html");
    fs::write(&index, page::render(&build.example, &font_name))
        .map_err(|source| Error::io(&index, source))?;
    println!("wrote {}", out.display());
    Ok(out)
}

/// Builds `build` and serves it on `port` until the process is stopped.
pub(crate) fn serve(root: &Path, what: &Build, port: u16) -> Result<()> {
    build(root, what)?;
    // Every example built so far is served, each under its own name.
    serve::run(&root.join("target/web"), &what.example, port)
}

/// Checks that the `wasm-bindgen` on the path is the version the lock file pins.
///
/// The bindings generator and the `wasm-bindgen` crate agree on a schema that changes between
/// versions, and a mismatch fails with a message that names neither.
fn check_bindgen(root: &Path) -> Result<()> {
    let lock = root.join("Cargo.lock");
    let text = fs::read_to_string(&lock).map_err(|source| Error::io(&lock, source))?;
    let wanted = locked_version(&text)
        .ok_or_else(|| Error::failed("Cargo.lock does not pin `wasm-bindgen`"))?;
    let install = format!("cargo install wasm-bindgen-cli --version {wanted} --locked");
    let found = process::capture(root, "wasm-bindgen", &["--version"])
        .map_err(|_| Error::failed(format!("`wasm-bindgen` is not installed; run `{install}`")))?;
    if found.split_whitespace().nth(1) == Some(wanted) {
        Ok(())
    } else {
        Err(Error::failed(format!(
            "`{}` is installed and Cargo.lock pins wasm-bindgen {wanted}; run `{install}`",
            found.trim()
        )))
    }
}

/// The version of the `wasm-bindgen` package in a lock file.
fn locked_version(lock: &str) -> Option<&str> {
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line == "name = \"wasm-bindgen\"" {
            return lines
                .next()?
                .strip_prefix("version = \"")?
                .strip_suffix('"');
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::locked_version;

    #[test]
    fn the_pinned_version_is_the_one_under_the_exact_package_name() {
        let lock = "[[package]]\nname = \"wasm-bindgen-futures\"\nversion = \"0.4.1\"\n\n\
                    [[package]]\nname = \"wasm-bindgen\"\nversion = \"0.2.130\"\n";
        assert_eq!(locked_version(lock), Some("0.2.130"));
        assert_eq!(locked_version("name = \"other\"\n"), None);
    }
}
