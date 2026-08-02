//! Building the component call a keyword stands for.
//!
//! Nothing here is new machinery: a keyword produces the same node an author writing the call by
//! hand would have produced, at the span of the keyword, so every message the call spelling raises
//! is raised against the keyword that asked for it.

use proc_macro2::Span;

use crate::view::attr::Attr;
use crate::view::attr::name::Name;
use crate::view::node::{Node, Tagged};
use crate::view::tag::Tag;
use crate::view::value::Value;

/// The component call a keyword is sugar for, written where the keyword was.
///
/// The name is a bare identifier, which is what the call spelling resolves, so a view using a
/// keyword needs the component of that name in scope exactly as one writing the call does.
pub(crate) fn call(name: &str, span: Span, attrs: Vec<Attr>, children: Vec<Node>) -> Tagged {
    let ident = syn::Ident::new(name, span);
    Tagged {
        tag: Tag::Component(syn::parse_quote!(#ident)),
        attrs,
        children,
        span,
    }
}

/// One `name = value` of that call.
pub(crate) fn prop(name: &str, value: Value) -> Attr {
    let name = Name {
        text: name.to_owned(),
        span: value.span,
    };
    Attr::Named { name, value }
}
