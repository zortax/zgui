//! The two Rust keywords a view answers to.
//!
//! `for` and `if` are the only lower-case words that begin a node rather than a call, and each is
//! sugar: it parses straight into the component call it stands for, so one lowering serves both
//! spellings and the two cannot drift apart.
//!
//! What makes the keywords safe is a rule about tokens rather than about types. The collection of
//! a `for` and the condition of an `if` are read again every time what they depend on changes, so
//! each is required to *be* a closure — `move`, `|` or `||` is the first token or the head is a
//! parse error. The head is then copied into the call verbatim, so a closure that was written
//! without `move` stays written without one.

mod body;
mod branch;
mod head;
mod list;
mod synth;

use syn::Token;
use syn::parse::ParseStream;

use crate::view::node::Tagged;

/// Whether what is written here is control flow rather than a call.
pub(crate) fn peek(input: ParseStream<'_>) -> bool {
    input.peek(Token![for]) || input.peek(Token![if])
}

/// Parses one control-flow node into the component call it is sugar for.
pub(crate) fn parse(input: ParseStream<'_>) -> syn::Result<Tagged> {
    if input.peek(Token![for]) {
        return list::parse(input);
    }
    branch::parse(input)
}
