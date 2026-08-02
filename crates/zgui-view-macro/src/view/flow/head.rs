//! The closure a reactive head is required to be.
//!
//! A collection or a condition that is read once is a snapshot, and a view built from a snapshot
//! never changes again. Requiring the closure syntactically — before a single token of the
//! expression is consumed — moves that mistake from a trait bound inside generated code onto the
//! word the author wrote, and leaves the non-reactive spelling with no way to be written at all.

use quote::ToTokens;
use syn::parse::ParseStream;
use syn::{Expr, Token};

use crate::view::value::Value;

/// Which head is being read, and therefore what the wrong spelling is told.
#[derive(Clone, Copy)]
pub(crate) enum Head {
    /// The collection of a `for`.
    Collection,
    /// The condition of an `if`.
    Condition,
}

/// Parses one head, which is a closure or nothing.
pub(crate) fn parse(input: ParseStream<'_>, head: Head) -> syn::Result<Value> {
    if !closure(input) {
        return Err(refuse(input, head));
    }
    let span = input.span();
    let expr = Expr::parse_without_eager_brace(input)?;
    Ok(Value { expr, span })
}

/// Whether the tokens about to be read begin a closure.
fn closure(input: ParseStream<'_>) -> bool {
    input.peek(Token![move]) || input.peek(Token![|]) || input.peek(Token![||])
}

/// The diagnostic for a head that was written as a value.
///
/// It reads the expression it is refusing so that the rewrite it prints is the author's own
/// expression with the closure around it, which is the whole of the fix.
fn refuse(input: ParseStream<'_>, head: Head) -> syn::Error {
    let span = input.span();
    let fork = input.fork();
    let written = Expr::parse_without_eager_brace(&fork).ok();
    let shown = written.as_ref().map_or_else(|| "…".to_owned(), source);
    let (headline, fix) = match head {
        Head::Collection => (
            "a list re-reads its collection, so `in` takes a closure",
            format!("help: write `in move || {shown}`"),
        ),
        Head::Condition => (
            "a condition is re-read when what it reads changes, so `if` takes a closure",
            format!("help: write `if move || {shown}`"),
        ),
    };
    let mut message = format!("{headline}\n\n");
    for note in notes(head, written.as_ref()) {
        message.push_str(&note);
        message.push('\n');
    }
    message.push_str(&fix);
    syn::Error::new(span, message)
}

/// The notes a particular wrong head earns.
///
/// A condition always earns the one about scope: the keyword resolves a name the author did not
/// write, and a view that cannot see it is owed the reason here rather than in a resolver error.
fn notes(head: Head, written: Option<&Expr>) -> Vec<String> {
    let mut notes = Vec::new();
    match (head, written) {
        (Head::Collection, Some(Expr::Range(_) | Expr::Reference(_) | Expr::Array(_))) => {
            notes.push(
                "note: `for` in a view is not Rust's `for`; it names a list that rebuilds itself"
                    .to_owned(),
            );
        }
        (Head::Condition, written) => {
            if let Some(path @ Expr::Path(_)) = written {
                let name = source(path);
                notes.push(format!(
                    "note: `{name}` may already be a closure; if it is, write \
                     `Show(when = {name}) {{ … }}`"
                ));
            }
            notes.push(SCOPE.to_owned());
        }
        _ => {}
    }
    notes
}

/// What an `if` costs a view that does not import the component it lowers to.
pub(crate) const SCOPE: &str = "note: `if` is written as `Show`, so the view needs `Show` in scope";

/// One expression as close to the way it was written as its tokens remember.
fn source(expr: &Expr) -> String {
    expr.to_token_stream().to_string().replace(' ', "")
}
