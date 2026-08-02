//! The files a new member starts life with.

use crate::new_crate::layer::Layer;

/// The manifest: version, edition, licence and lints all inherited from the workspace, and no
/// dependency table to tempt anyone into pinning a version locally.
pub(crate) fn manifest(name: &str, layer: Layer) -> String {
    format!(
        "[package]\n\
         name = \"{name}\"\n\
         version.workspace = true\n\
         edition.workspace = true\n\
         license.workspace = true\n\
         \n\
         # {layer} — {description}. Every dependency is inherited: `foo.workspace = true`.\n\
         [dependencies]\n\
         \n\
         [lints]\n\
         workspace = true\n",
        description = layer.description()
    )
}

/// The crate root: the lint header every published crate carries, and a doc comment that stands
/// on its own.
pub(crate) fn crate_root(name: &str, layer: Layer) -> String {
    let title = name.strip_prefix("zgui-").unwrap_or(name).replace('-', " ");
    format!(
        "//! The {title} crate.\n\
         //!\n\
         //! Layer {layer}: {description}.\n\
         \n\
         #![deny(missing_docs)]\n\
         #![forbid(unsafe_code)]\n",
        description = layer.description()
    )
}
