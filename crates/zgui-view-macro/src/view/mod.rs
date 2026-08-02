//! The `view!` grammar.
//!
//! A node is written as the thing it names, an attribute list in parentheses, and its children in
//! a block: `row(class = "a") { "hi" }`. Both parts are optional and at least one is required, so
//! `row()` and `row { "hi" }` are nodes and `row` on its own is not.
//!
//! Every group a node is made of is a delimiter the lexer has already balanced, which is why
//! nothing in here is found by scanning.

mod attr;
mod attrs;
mod flow;
mod lower;
mod node;
mod refuse;
mod reserved;
mod tag;
mod value;

use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};

use crate::view::node::Node;

/// One `view!` invocation.
struct View {
    /// Its root nodes. More than one is a fragment.
    roots: Vec<Node>,
}

impl Parse for View {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        Ok(Self {
            roots: node::siblings(input)?,
        })
    }
}

/// Parses and lowers one invocation.
pub(crate) fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let view = syn::parse2::<View>(input)?;
    lower::lower(&view.roots)
}

#[cfg(test)]
mod tests;
