//! What sits between two tags.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};

use crate::view::lower::{component, element};
use crate::view::node::Node;

/// Lowers one child to the value it contributes.
pub(crate) fn lower(node: &Node) -> syn::Result<TokenStream> {
    match node {
        Node::Text(text) => Ok(quote_spanned!(text.span()=> #text)),
        Node::Block(expr) => Ok(quote!(#expr)),
        Node::Tagged(tagged) if tagged.tag.is_component() => component::lower(tagged),
        Node::Tagged(tagged) => element::lower(tagged),
    }
}

/// Lowers a list of children to one value: nothing, the child itself, or a tuple.
///
/// A tuple is a view, so a fragment costs no allocation and every child keeps its own type.
pub(crate) fn lower_all(nodes: &[&Node]) -> syn::Result<TokenStream> {
    let lowered = nodes
        .iter()
        .map(|node| lower(node))
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(match lowered.len() {
        0 => quote!(()),
        1 => lowered.into_iter().next().expect("one child"),
        // Each member is converted on the way in. A tuple is a view only when every member is one,
        // and a component's own return type is `impl IntoView` rather than `impl View` — so a
        // fragment of two component calls would otherwise be the one shape of children that does
        // not compile, which is exactly what a list of items inside a group is.
        _ => {
            let converted = lowered
                .iter()
                .map(|child| quote!(::zgui::expansion::view::IntoView::into_view(#child)));
            quote!((#(#converted,)*))
        }
    })
}
