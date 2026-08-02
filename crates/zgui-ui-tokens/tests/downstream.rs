//! The constraint this crate exists to prove: it is an ordinary downstream consumer.
//!
//! The component library may not reach into framework internals, because anything it needs an
//! application author needs too. That is a rule about *this crate's* sources and manifest, so it is
//! checked here rather than described somewhere.

use std::path::{Path, PathBuf};

/// The crates below the public API. Naming one of these here would mean a behaviour that an
/// application author could not have written.
const INTERNAL: [&str; 12] = [
    "zgui_input",
    "zgui-input",
    "zgui_scroll",
    "zgui-scroll",
    "zgui_layout",
    "zgui-layout",
    "zgui_dom",
    "zgui-dom",
    "zgui_style",
    "zgui-style",
    "zgui_paint",
    "zgui-paint",
];

/// This crate's directory.
fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` file under `dir`.
fn sources(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            into.push(path);
        }
    }
}

#[test]
fn the_sources_name_no_crate_below_the_public_api() {
    let mut files = Vec::new();
    sources(&crate_dir().join("src"), &mut files);
    assert!(!files.is_empty(), "no sources were found to check");

    for file in files {
        let text = std::fs::read_to_string(&file).expect("the source is readable");
        for name in INTERNAL {
            assert!(
                !text.contains(name),
                "{} names `{name}`, which is below the public API",
                file.display()
            );
        }
    }
}

#[test]
fn the_manifest_depends_on_the_umbrella_crate_and_nothing_else() {
    let manifest =
        std::fs::read_to_string(crate_dir().join("Cargo.toml")).expect("the manifest is readable");
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("the manifest has a dependency table")
        .split('[')
        .next()
        .expect("the table ends");

    let named: Vec<&str> = dependencies
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    assert_eq!(
        named.len(),
        1,
        "a second dependency here would be a hole in the public API rather than a convenience: \
         {named:?}"
    );
    assert!(named[0].starts_with("zgui ="), "{}", named[0]);
}
