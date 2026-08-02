//! Turns the style engine's generated property list into a table this crate can hand out.
//!
//! The engine generates one property module per property it was built with, and beside it a
//! machine-readable list of every property name and the preference gating it. That list is the only
//! honest denominator for a parity count: it holds exactly what *this* build of the engine
//! generated, so it moves when the engine's configuration moves, and a hand-written copy of it
//! would not.
//!
//! The engine publishes the directory it generated into, and this crate is one of the three allowed
//! to name the engine at all, so the reading happens here and the result is a plain table anything
//! may read.

#[path = "build/json.rs"]
mod json;

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

/// Reads the generated list and writes the table.
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=build/json.rs");

    let source = source_path();
    println!("cargo::rerun-if-changed={}", source.display());
    let text = fs::read_to_string(&source).unwrap_or_else(|error| {
        panic!(
            "the style engine's property list at {} could not be read: {error}",
            source.display()
        )
    });
    let catalog = json::parse(&text).unwrap_or_else(|reason| {
        panic!("{} is not the expected shape: {reason}", source.display())
    });
    assert!(
        !catalog.longhands.is_empty() && !catalog.shorthands.is_empty(),
        "{} lists no properties, so every parity count taken against it would be vacuous",
        source.display(),
    );

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    fs::write(out.join("catalog.rs"), render(&catalog)).expect("the table could be written");
}

/// Where the engine said it generated its properties.
///
/// The engine declares a native library name and prints its output directory, so cargo hands that
/// directory to the build script of every crate that depends on it directly. Nothing here guesses
/// at a path inside the build directory.
fn source_path() -> PathBuf {
    let out_dir = env::var_os("DEP_SERVO_STYLE_CRATE_OUT_DIR").expect(
        "the style engine publishes its output directory to its direct dependents; this crate is \
         one of them",
    );
    PathBuf::from(out_dir).join("css-properties.json")
}

/// Renders the table as Rust source.
fn render(catalog: &json::Catalog) -> String {
    let mut out = String::new();
    out.push_str("// Generated from the style engine's own property list. Do not edit.\n");
    section(&mut out, "LONGHAND_TABLE", &catalog.longhands);
    section(&mut out, "SHORTHAND_TABLE", &catalog.shorthands);
    out
}

/// Renders one named table.
fn section(out: &mut String, name: &str, entries: &[json::Entry]) {
    let _ = writeln!(
        out,
        "pub(crate) static {name}: [(&str, Option<&str>); {}] = [",
        entries.len()
    );
    for entry in entries {
        let pref = match &entry.pref {
            None => "None".to_owned(),
            Some(pref) => format!("Some({pref:?})"),
        };
        let _ = writeln!(out, "    ({:?}, {pref}),", entry.name);
    }
    out.push_str("];\n");
}
