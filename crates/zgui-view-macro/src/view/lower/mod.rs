//! Turning a parsed view into the builder chain it stands for.
//!
//! The expansion is thin on purpose: every attribute becomes one call on a builder, so the types
//! carry the meaning, an editor completes attribute names, and a mistake is reported against the
//! method it names rather than inside a macro.

mod a11y;
mod bundle;
mod children;
mod component;
mod element;

use proc_macro2::TokenStream;
use quote::quote;

use crate::view::node::Node;

/// Lowers the roots of one `view!` invocation.
///
/// More than one root is a fragment, which is a tuple: each child keeps its own type, and nothing
/// is allocated to hold them together.
pub(crate) fn lower(roots: &[Node]) -> syn::Result<TokenStream> {
    let roots: Vec<&Node> = roots.iter().collect();
    let lowered = children::lower_all(&roots)?;
    Ok(quote!(#lowered))
}
