//! The shape of the public surface, checked against the sources that produce it.
//!
//! Two rules, both about what happens to a downstream consumer when this contract grows.
//!
//! Every public enumeration is extensible. This crate describes platforms, and platforms acquire
//! capabilities: a new pointer device, a new clipboard representation, a new reason for the loop to
//! wake. A backend or an application that matched exhaustively on one of these today has to keep
//! compiling when that happens, and marking the enumerations is what buys it.
//!
//! Nothing here names a windowing library or a graphics API. That is the whole contract — the seam
//! exists so that the one part of a migration or a port that would otherwise reach every crate in
//! the tree is confined to a backend — and it is worth checking rather than trusting, because the
//! way it gets broken is a single convenient import.

use std::fs;
use std::path::{Path, PathBuf};

/// Every `.rs` file under the crate's source directory.
fn sources() -> Vec<PathBuf> {
    fn walk(directory: &Path, into: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(directory).expect("the source directory is readable");
        for entry in entries {
            let path = entry.expect("the directory entry is readable").path();
            if path.is_dir() {
                walk(&path, into);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                into.push(path);
            }
        }
    }

    let mut files = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    files.sort();
    files
}

/// The declarations of public enumerations that are not marked extensible.
fn unmarked_enums(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut unmarked = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("pub enum ") {
            continue;
        }
        let mut marked = false;
        for earlier in lines[..index].iter().rev() {
            let earlier = earlier.trim_start();
            if earlier == "#[non_exhaustive]" {
                marked = true;
                break;
            }
            let is_preamble = earlier.starts_with("#[")
                || earlier.starts_with("///")
                || earlier.starts_with("//!")
                || earlier.starts_with("//")
                || earlier.is_empty()
                || earlier.starts_with(')')
                || earlier.starts_with(']');
            if !is_preamble {
                break;
            }
        }
        if !marked {
            unmarked.push((*line).trim().to_owned());
        }
    }
    unmarked
}

#[test]
fn every_public_enumeration_is_extensible() {
    let mut offenders = Vec::new();
    for path in sources() {
        let text = fs::read_to_string(&path).expect("the source file is readable");
        for declaration in unmarked_enums(&text) {
            offenders.push(format!("{}: {declaration}", path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "these public enumerations are not marked `#[non_exhaustive]`:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_check_finds_an_unmarked_enumeration() {
    let marked = "/// Doc.\n#[non_exhaustive]\n#[derive(Debug)]\npub enum Marked { A }\n";
    let unmarked = "/// Doc.\n#[derive(Debug)]\npub enum Unmarked { A }\n";
    assert!(unmarked_enums(marked).is_empty());
    assert_eq!(unmarked_enums(unmarked), vec!["pub enum Unmarked { A }"]);
}

#[test]
fn no_source_names_a_windowing_library_or_a_graphics_api() {
    // The list is matched against import statements only, so writing it here does not trip it.
    let forbidden = [
        "winit",
        "wgpu",
        "glutin",
        "vulkano",
        "web_sys",
        "sdl2",
        "gtk",
        "softbuffer",
    ];

    let mut offenders = Vec::new();
    for path in sources() {
        let text = fs::read_to_string(&path).expect("the source file is readable");
        for line in text.lines() {
            let trimmed = line.trim_start();
            let import = trimmed
                .strip_prefix("pub use ")
                .or_else(|| trimmed.strip_prefix("use "));
            let Some(import) = import else { continue };
            let root = import
                .split([':', ' ', ';', '{'])
                .next()
                .unwrap_or_default();
            if forbidden.contains(&root) {
                offenders.push(format!("{}: {}", path.display(), trimmed));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the platform contract must name no windowing library and no graphics api:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_check_reads_more_than_one_source_file() {
    // A scanner pointed at nothing passes vacuously, so the corpus itself is asserted.
    assert!(sources().len() > 10);
}
