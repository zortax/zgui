//! `cargo xtask new-crate` — a member that satisfies every ledger before it holds any code.
//!
//! A 44-crate workspace makes the cost of getting a manifest header wrong recur 44 times. The
//! template is the mitigation, and it is available from the first day rather than the day
//! someone notices.

pub(crate) mod layer;
pub(crate) mod template;

use std::path::Path;

pub(crate) use crate::new_crate::layer::Layer;

use crate::error::{Error, Result, write};

/// Creates `crates/<name>` from the template.
pub(crate) fn run(root: &Path, name: &str, layer: Layer) -> Result<()> {
    validate(name)?;
    let directory = root.join("crates").join(name);
    if directory.exists() {
        return Err(Error::failed(format!(
            "crates/{name} already exists; nothing was written"
        )));
    }
    write(
        directory.join("Cargo.toml"),
        &template::manifest(name, layer),
    )?;
    write(
        directory.join("src/lib.rs"),
        &template::crate_root(name, layer),
    )?;
    println!("created crates/{name} ({layer})");
    println!("  crates/{name}/Cargo.toml");
    println!("  crates/{name}/src/lib.rs");
    println!(
        "the workspace globs crates/*, so nothing else needs editing; \
         add `{name}` to the phase that introduces it in docs/planning/PHASES.md or \
         `cargo xtask ledger topo` will say so"
    );
    Ok(())
}

/// Rejects names that would fail a ledger the moment they were created.
fn validate(name: &str) -> Result<()> {
    if !name.starts_with("zgui-") {
        return Err(Error::failed(format!(
            "`{name}` must start with `zgui-`: every published member does"
        )));
    }
    let shape_is_good = name.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    });
    if !shape_is_good {
        return Err(Error::failed(format!(
            "`{name}` must be lowercase ASCII with hyphens"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Layer, template, validate};

    #[test]
    fn rejects_names_a_ledger_would_reject() {
        assert!(validate("zgui-paint").is_ok());
        assert!(validate("paint").is_err());
        assert!(validate("zgui_paint").is_err());
        assert!(validate("zgui-Paint").is_err());
    }

    #[test]
    fn the_template_carries_the_lint_header() {
        let root = template::crate_root("zgui-paint", Layer::L4);
        assert!(root.contains("#![deny(missing_docs)]"));
        assert!(root.contains("#![forbid(unsafe_code)]"));
        let manifest = template::manifest("zgui-paint", Layer::L4);
        assert!(manifest.contains("[lints]\nworkspace = true"));
    }
}
