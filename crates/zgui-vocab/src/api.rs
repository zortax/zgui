//! The shape of the public surface, checked against the sources that produce it.
//!
//! One rule here, and it is a rule about the future rather than about the present: every public
//! enumeration in this crate is extensible. A vocabulary is the one kind of crate that grows —
//! a new event, a new pointer device, a new way for a value to change — and a downstream match
//! that compiled today has to keep compiling when it does. Marking the enumerations extensible is
//! what buys that, and forgetting to mark one is invisible until the day it breaks somebody.
//!
//! So the check reads the crate's own text. It is coarse on purpose: anything that looks like a
//! public enumeration counts, including one produced by a macro, because that is exactly the case
//! a type-level check would miss.

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
///
/// A declaration is preceded by its attributes and its documentation, so the search walks
/// backwards over those and stops at the first line that is neither.
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
fn the_check_reads_more_than_one_source_file() {
    // A scanner pointed at nothing passes vacuously, so the corpus itself is asserted.
    assert!(sources().len() > 10);
}
