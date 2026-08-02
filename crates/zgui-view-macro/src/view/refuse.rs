//! What node position refuses, and what it says instead.
//!
//! Each message answers the same question — this is not a node, so what was meant? — by naming a
//! rewrite that works rather than the category that was expected. Two of them exist because the
//! obvious rewrite is wrong: a macro call cannot become an element, and a bundle of attributes has
//! a home one delimiter away.

use proc_macro2::Span;
use syn::parse::ParseStream;
use syn::{Token, token};

/// The diagnostic for a name written where a node was expected.
///
/// A name followed by `!` is a macro call, and the advice a plain name gets — call it, and it is a
/// childless element — resolves to an element of that name and cannot compile. So the two are told
/// apart, and each is given the rewrite that works.
pub(crate) fn bare(name: &str, span: Span, is_macro: bool) -> syn::Error {
    if is_macro {
        return syn::Error::new(
            span,
            format!(
                "`{name}!` is a macro call, not a node\n\n\
                 help: a macro call is a value, so it goes in braces: `{{{name}!(…)}}`"
            ),
        );
    }
    syn::Error::new(
        span,
        format!(
            "`{name}` is a name, not a node\n\n\
             help: text is a string literal `\"{name}\"`; a value goes in braces: `{{{name}}}`; \
             a childless element is written `{name}()`; children go in a block: `{name} {{ … }}`"
        ),
    )
}

/// The diagnostic for a token tree that begins no node at all.
pub(crate) fn unexpected(input: ParseStream<'_>) -> syn::Error {
    let span = input.span();
    if input.peek(token::Paren) {
        return syn::Error::new(
            span,
            "`(` cannot begin a node\n\n\
             note: a node takes one attribute list, immediately after its name",
        );
    }
    if input.peek(Token![<]) {
        return syn::Error::new(
            span,
            "`<` cannot begin a node\n\n\
             help: a node is a call and a block: `Button(class = \"x\") { \"Save\" }`",
        );
    }
    if input.peek(Token![..]) {
        return spread(span);
    }
    syn::Error::new(
        span,
        "expected a node: an element `row()`, a component `Button()`, text `\"…\"`, or a braced \
         expression `{…}`\n\n\
         note: text is a string literal, because a bare word would make every Rust expression \
         ambiguous",
    )
}

/// The diagnostic for a spread written among the children.
///
/// A bundle is a range expression to every parser that reads one, so the mistake compiles far
/// enough to fail against a trait, naming a type nobody wrote. Refusing it here costs one peek and
/// the answer is a delimiter away.
pub(crate) fn spread(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "a spread forwards a bundle of attributes, so it goes in the attribute list\n\n\
         help: write it between the parentheses: `Card({..attrs})`",
    )
}
