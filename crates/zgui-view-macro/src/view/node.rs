//! The three things a view is made of, and which one is written here.
//!
//! In node position the first token tree settles the question and nothing after it can revise the
//! answer: a string literal is text, a brace group is a value, `for` and `if` are control flow,
//! and any other identifier begins a call. Everything else is an error, so a bare Rust expression
//! is not a child and cannot be mistaken for one.
//!
//! Having read what a call names, one more token tree finishes the decision: `(` opens its
//! attribute list, `{` opens its children, and neither means the name was written on its own,
//! which is not a node.

use proc_macro2::Span;
use syn::ext::IdentExt;
use syn::parse::ParseStream;
use syn::{Token, braced, parenthesized, token};

use crate::view::attr::Attr;
use crate::view::tag::Tag;
use crate::view::{attrs, flow, refuse, reserved};

/// One node of a view.
pub(crate) enum Node {
    /// An element or a component, with what was written in its attribute list and its block.
    Tagged(Tagged),
    /// A string literal.
    Text(syn::LitStr),
    /// A braced expression: anything that converts into a view.
    Block(syn::Expr),
}

/// An element or component call, with its attributes and children.
pub(crate) struct Tagged {
    /// What the call named.
    pub(crate) tag: Tag,
    /// Everything written in the attribute list.
    pub(crate) attrs: Vec<Attr>,
    /// Everything written in the block.
    pub(crate) children: Vec<Node>,
    /// Where the name was written.
    pub(crate) span: Span,
}

/// Parses nodes until the stream runs out.
pub(crate) fn siblings(input: ParseStream<'_>) -> syn::Result<Vec<Node>> {
    let mut nodes = Vec::new();
    while !input.is_empty() {
        nodes.push(parse(input)?);
    }
    Ok(nodes)
}

/// Parses one node.
fn parse(input: ParseStream<'_>) -> syn::Result<Node> {
    if input.peek(syn::LitStr) {
        return Ok(Node::Text(input.parse()?));
    }
    if input.peek(token::Brace) {
        return block(input);
    }
    if flow::peek(input) {
        return Ok(Node::Tagged(flow::parse(input)?));
    }
    if input.peek(syn::Ident::peek_any) {
        return Ok(Node::Tagged(call(input)?));
    }
    Err(refuse::unexpected(input))
}

/// Parses `{ expression }`.
fn block(input: ParseStream<'_>) -> syn::Result<Node> {
    let content;
    let brace = braced!(content in input);
    if content.peek(Token![..]) {
        return Err(refuse::spread(brace.span.join()));
    }
    let expr = content.parse::<syn::Expr>()?;
    if !content.is_empty() {
        return Err(syn::Error::new(
            content.span(),
            "a braced child is one expression",
        ));
    }
    Ok(Node::Block(expr))
}

/// Parses `name(…)`, `name { … }` or `name(…) { … }`.
fn call(input: ParseStream<'_>) -> syn::Result<Tagged> {
    reserved::check(input)?;
    let tag = Tag::parse(input)?;
    let span = tag.span();
    let mut attributes = Vec::new();
    let mut children = Vec::new();
    let mut written = false;

    if input.peek(token::Paren) {
        let content;
        parenthesized!(content in input);
        attributes = attrs::list(&content)?;
        written = true;
    }
    // A `{` here is this call's children, whatever whitespace stands before it: a proc macro cannot
    // see the difference between `) {` and `)` on one line and `{` on the next. A childless call
    // followed by a braced sibling therefore writes its empty block, `row() {} {expr}`.
    if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        children = siblings(&content)?;
        written = true;
    }

    if !written {
        return Err(refuse::bare(&tag.text(), span, input.peek(Token![!])));
    }
    Ok(Tagged {
        tag,
        attrs: attributes,
        children,
        span,
    })
}
