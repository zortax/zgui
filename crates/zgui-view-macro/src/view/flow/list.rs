//! `for row in <closure>, key = <value> { … }`.
//!
//! The row is one name, the collection is a closure, and the key is required — the three things
//! the list component asks for, in the order a Rust reader looks for them.

use proc_macro2::Span;
use syn::parse::ParseStream;
use syn::{Token, token};

use crate::view::attr::Attr;
use crate::view::flow::body::{self, Body};
use crate::view::flow::head::{self, Head};
use crate::view::flow::synth;
use crate::view::node::Tagged;
use crate::view::value::Value;

/// Parses one `for`.
pub(crate) fn parse(input: ParseStream<'_>) -> syn::Result<Tagged> {
    let keyword = input.parse::<Token![for]>()?;
    let row = row(input)?;
    collection(input)?;
    let each = head::parse(input, Head::Collection)?;
    let key = key(input)?;
    input.parse::<Option<Token![,]>>()?;
    let children = body::parse(input, Body::Row)?;
    if input.peek(Token![else]) {
        return Err(no_alternative(input.span()));
    }
    let attrs = vec![
        synth::prop("each", each),
        synth::prop("key", key),
        Attr::Let(row),
    ];
    Ok(synth::call("For", keyword.span, attrs, children))
}

/// Parses the name the row is bound to.
fn row(input: ParseStream<'_>) -> syn::Result<syn::Ident> {
    let span = input.span();
    if input.peek(token::Paren) || input.peek(Token![_]) {
        return Err(one_name(span));
    }
    input.parse::<syn::Ident>().map_err(|_| one_name(span))
}

/// Reads the `in` that introduces the collection.
fn collection(input: ParseStream<'_>) -> syn::Result<()> {
    let span = input.span();
    input.parse::<Token![in]>().map_err(|_| {
        syn::Error::new(
            span,
            "a `for` names its collection after `in`: `for row in move || rows.get(), key = k`",
        )
    })?;
    Ok(())
}

/// Parses the `, key = …` the collection is followed by.
fn key(input: ParseStream<'_>) -> syn::Result<Value> {
    let span = input.span();
    if input.parse::<Option<Token![,]>>()?.is_none() {
        return Err(no_key(span));
    }
    let fork = input.fork();
    let named = fork.parse::<syn::Ident>().is_ok_and(|name| name == "key");
    if !named {
        return Err(no_key(input.span()));
    }
    let name = input.parse::<syn::Ident>()?;
    input
        .parse::<Token![=]>()
        .map_err(|_| no_key(name.span()))?;
    Value::parse(input)
}

/// The diagnostic for a row bound to something other than one name.
fn one_name(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "a row binds one name\n\n\
         help: bind the row and destructure in the body",
    )
}

/// The diagnostic for a list with no key.
fn no_key(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "a list needs a key, so a row keeps its identity when the collection changes\n\n\
         help: `for row in c, key = |row: &Row| row.id { … }`",
    )
}

/// The diagnostic for an alternative written after a list.
fn no_alternative(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "`for` has no `else`\n\n\
         note: an empty list renders nothing; to show something instead, wrap it in \
         `if move || !rows.get().is_empty() { … } else { … }`",
    )
}
