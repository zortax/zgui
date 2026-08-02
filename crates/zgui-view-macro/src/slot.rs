//! `#[slot]`: a named group of children a component takes beside its ordinary ones.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemStruct;

use crate::component::{builder, prop};

/// Expands `#[slot]`.
///
/// A slot *is* its own props: the builder generated here is what a `<CardHeader slot>` child
/// expands to, and the value it builds is what the component's prop holds.
pub(crate) fn expand(attribute: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    if !attribute.is_empty() {
        return Err(syn::Error::new_spanned(
            attribute,
            "`#[slot]` takes no options",
        ));
    }
    let declaration = syn::parse2::<ItemStruct>(item)?;
    let name = declaration.ident.clone();
    let (props, visibilities) = prop::from_fields(&declaration.fields)?;
    let attributes = &declaration.attrs;
    builder::generate(
        &declaration.vis,
        &name,
        &name,
        &declaration.generics,
        &quote!(#(#attributes)*),
        &props,
        Some(&visibilities),
    )
}
