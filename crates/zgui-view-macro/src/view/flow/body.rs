//! The block a keyword renders.
//!
//! An empty block is refused here rather than left to the props builder, because the component a
//! keyword lowers to asks for its children through a trait bound, and a bound that goes unmet
//! names a type the author never wrote.

use proc_macro2::Span;
use syn::parse::ParseStream;
use syn::{braced, token};

use crate::view::flow::head;
use crate::view::node::{self, Node};

/// Which block is being read, and therefore what an empty one is told.
#[derive(Clone, Copy)]
pub(crate) enum Body {
    /// The row of a `for`: what one item looks like.
    Row,
    /// The body of an `if`: what is shown while the condition holds.
    Shown,
}

/// Parses one block, which is written and is not empty.
pub(crate) fn parse(input: ParseStream<'_>, body: Body) -> syn::Result<Vec<Node>> {
    if input.peek(token::Paren) {
        return Err(attributes(input.span()));
    }
    if !input.peek(token::Brace) {
        return Err(missing(input.span(), body));
    }
    let content;
    let brace = braced!(content in input);
    let nodes = node::siblings(&content)?;
    if nodes.is_empty() {
        return Err(empty(brace.span.join(), body));
    }
    Ok(nodes)
}

/// Parses the block of an `else`, which may be empty and whose contents stay as tokens.
///
/// What an `else` holds is a view of its own, so it is handed back to the macro rather than read
/// here: the alternative branch is then the same program whether it was written after `else` or
/// passed to the component as a fallback.
pub(crate) fn alternative(input: ParseStream<'_>) -> syn::Result<(Span, proc_macro2::TokenStream)> {
    if !input.peek(token::Brace) {
        return Err(chained(input.span()));
    }
    let content;
    let brace = braced!(content in input);
    Ok((brace.span.join(), content.parse()?))
}

/// The diagnostic for an attribute list written on control flow.
fn attributes(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "control flow takes no attributes\n\n\
         note: attributes belong on an element inside the body",
    )
}

/// The diagnostic for a block that was never opened.
fn missing(span: Span, body: Body) -> syn::Error {
    let message = match body {
        Body::Row => {
            "a `for` renders its row in a block: `for row in move || rows.get(), key = k { … }`"
        }
        Body::Shown => "an `if` shows its body in a block: `if move || open.get() { … }`",
    };
    syn::Error::new(span, message.to_owned())
}

/// The diagnostic for a block with nothing in it.
fn empty(span: Span, body: Body) -> syn::Error {
    let message = match body {
        Body::Row => "a `for` needs a row: what one item looks like".to_owned(),
        Body::Shown => format!(
            "an `if` needs a body: what is shown while the condition holds\n\n{}",
            head::SCOPE
        ),
    };
    syn::Error::new(span, message)
}

/// The diagnostic for a conditional chained onto another one.
fn chained(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "`else` takes a block\n\n\
         note: each arm is its own conditional, and an outer arm changing rebuilds the ones \
         inside it\n\
         help: write `else { if move || … { … } else { … } }`",
    )
}
