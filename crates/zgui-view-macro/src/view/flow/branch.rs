//! `if <closure> { … } else { … }`.
//!
//! The two branches sit side by side, at one indentation, in the order they are chosen. An `else`
//! with nothing in it is the alternative that renders nothing, and is written as such rather than
//! dropped, so a conditional keeps saying which of its two spellings the author chose.

use proc_macro2::{Span, TokenStream};
use syn::parse::ParseStream;
use syn::{Expr, Token};

use crate::view::attr::Attr;
use crate::view::flow::body::{self, Body};
use crate::view::flow::head::{self, Head};
use crate::view::flow::synth;
use crate::view::node::Tagged;
use crate::view::value::Value;

/// Parses one `if`.
pub(crate) fn parse(input: ParseStream<'_>) -> syn::Result<Tagged> {
    let keyword = input.parse::<Token![if]>()?;
    if input.peek(Token![let]) {
        return Err(binding(input.span()));
    }
    let when = head::parse(input, Head::Condition)?;
    let children = body::parse(input, Body::Shown)?;
    let mut attrs: Vec<Attr> = vec![synth::prop("when", when)];
    if input.peek(Token![else]) {
        input.parse::<Token![else]>()?;
        let (span, tokens) = body::alternative(input)?;
        attrs.push(synth::prop("fallback", alternative(span, tokens)));
    }
    Ok(synth::call("Show", keyword.span, attrs, children))
}

/// The alternative branch, as the thunk the component is given.
///
/// An empty one captures nothing and says so; a written one captures whatever its body reads, and
/// is handed back to the macro as a view of its own so that the branch not taken is compiled from
/// the same grammar as the branch taken.
fn alternative(span: Span, tokens: TokenStream) -> Value {
    let expr: Expr = if tokens.is_empty() {
        syn::parse_quote_spanned!(span=> || ())
    } else {
        let view = syn::Ident::new("view", span);
        syn::parse_quote_spanned!(span=> move || #view! { #tokens })
    };
    Value { expr, span }
}

/// The diagnostic for a pattern match written as a condition.
fn binding(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "`if let` is not part of the view grammar\n\n\
         help: compute it outside the view, or use a braced child with a `match`",
    )
}
