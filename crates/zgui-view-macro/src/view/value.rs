//! What an attribute is given.

use proc_macro2::Span;
use quote::{ToTokens, TokenStreamExt};
use syn::parse::ParseStream;
use syn::{Expr, Stmt};

/// One attribute's value: a literal, a path, a closure, or a braced expression.
///
/// A value ends where its expression ends, and the `,` or `)` that follows it belongs to the
/// attribute list around it. Nothing about what an expression may contain is therefore restricted:
/// a comparison, a shift and a generic argument list are all values.
#[derive(Clone)]
pub(crate) struct Value {
    /// The expression the value's tokens parsed as.
    pub(crate) expr: Expr,
    /// Where the value was written.
    pub(crate) span: Span,
}

impl Value {
    /// Parses one value, leaving whatever terminates it unconsumed.
    ///
    /// A brace begins a block rather than a struct literal: an attribute list whose comma has gone
    /// missing is then a parse error rather than a functional update of the value before it, which
    /// is a mistake worth failing loudly on.
    pub(crate) fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let span = input.span();
        let expr = Expr::parse_without_eager_brace(input).map_err(not_an_expression)?;
        Ok(Self {
            expr: unwrap_block(expr),
            span,
        })
    }

    /// A value that stands for the identifier of the same name, for the `name` shorthand.
    pub(crate) fn shorthand(name: &str, span: Span) -> syn::Result<Self> {
        if !name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
            return Err(syn::Error::new(
                span,
                format!(
                    "`{name}` is not an identifier, so there is no variable of that name to \
                     stand in for it\n\n\
                     help: give it a value: `{name}={{…}}`"
                ),
            ));
        }
        let ident = syn::Ident::new(name, span);
        Ok(Self {
            expr: syn::parse_quote!(#ident),
            span,
        })
    }
}

impl ToTokens for Value {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.append_all(self.expr.to_token_stream());
    }
}

/// Frames what the expression parser refused, so the error says which part of the view it is in.
///
/// The tokens it read are the author's own and its span points into them, so the reading it names
/// — a chained comparison where a generic argument list was meant, say — is kept and told where it
/// was written.
fn not_an_expression(error: syn::Error) -> syn::Error {
    syn::Error::new(
        error.span(),
        format!("an attribute value is one expression, and this is not one: {error}"),
    )
}

/// Unwraps `{ expr }` to `expr`, so a braced value expands to what was written inside it.
fn unwrap_block(expr: Expr) -> Expr {
    let Expr::Block(block) = &expr else {
        return expr;
    };
    if block.label.is_some() || !block.attrs.is_empty() || block.block.stmts.len() != 1 {
        return expr;
    }
    match &block.block.stmts[0] {
        Stmt::Expr(inner, None) => inner.clone(),
        _ => expr,
    }
}
