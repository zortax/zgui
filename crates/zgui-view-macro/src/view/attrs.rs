//! What a node is given, between its parentheses.
//!
//! The list is read one attribute at a time rather than by a terminated parser, because the two
//! ways it goes wrong are worth telling apart: a `{` here is a struct literal whose braces the
//! value parser refused to eat, and anything else is a comma that was never written. Both are
//! likelier than they were, since a value now ends where its expression ends and a separator is
//! the only thing that says the next attribute has begun.

use proc_macro2::Span;
use quote::ToTokens;
use syn::parse::ParseStream;
use syn::{Token, braced, token};

use crate::view::attr::Attr;

/// Parses a whole attribute list, which may carry a trailing comma.
pub(crate) fn list(input: ParseStream<'_>) -> syn::Result<Vec<Attr>> {
    let mut attrs = Vec::new();
    while !input.is_empty() {
        attrs.push(Attr::parse(input)?);
        if input.is_empty() {
            break;
        }
        if input.peek(token::Brace) {
            return Err(braced_after_a_value(input, attrs.last())?);
        }
        if input.parse::<Option<Token![,]>>()?.is_none() {
            return Err(missing_comma(input.span()));
        }
    }
    Ok(attrs)
}

/// The diagnostic for a brace group standing where a comma was expected.
///
/// A value is one expression and a `{` does not extend one, so the braces of a struct literal are
/// left behind unread — as is a spread whose comma went missing, which is the same shape and a
/// different mistake.
fn braced_after_a_value(input: ParseStream<'_>, last: Option<&Attr>) -> syn::Result<syn::Error> {
    let fork = input.fork();
    let content;
    let brace = braced!(content in fork);
    if content.peek(Token![..]) {
        return Ok(missing_comma(brace.span.join()));
    }
    Ok(struct_literal(last, brace.span.join()))
}

/// The diagnostic for two attributes written with nothing between them.
fn missing_comma(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "attributes are separated by commas\n\n\
         help: write one between them: `class = \"a\", on:click = f`",
    )
}

/// The diagnostic for a struct literal written as a value without braces.
fn struct_literal(last: Option<&Attr>, span: Span) -> syn::Error {
    let rewrite = match last {
        Some(Attr::Named { name, value }) => format!(
            "{} = {{{} {{ … }}}}",
            name.text,
            value.expr.to_token_stream()
        ),
        _ => "at = {Point { x, y }}".to_owned(),
    };
    syn::Error::new(
        span,
        format!(
            "a struct literal in an attribute value goes in braces\n\n\
             note: a value is one expression, and a `{{` after one opens a block rather than \
             extending it\n\
             help: write `{rewrite}`"
        ),
    )
}
