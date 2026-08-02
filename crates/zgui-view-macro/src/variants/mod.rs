//! `variants!`: a component's axes of visual variation.

mod emit;
mod table;

use proc_macro2::TokenStream;

/// Expands `variants!`.
pub(crate) fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let table = syn::parse2::<table::Table>(input)?;
    Ok(emit::emit(&table))
}
