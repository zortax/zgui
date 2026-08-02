//! What a node names.

use proc_macro2::Span;
use quote::ToTokens;
use syn::Token;
use syn::parse::ParseStream;

use crate::view::attr::name::Name;

/// What a node names: an element of the intrinsic vocabulary, an element from somewhere else, or
/// a component.
pub(crate) enum Tag {
    /// `row(…)`: the intrinsic vocabulary, resolved to a builder of the same name.
    Intrinsic(Name),
    /// `html::div(…)`: an element vocabulary of someone else's, resolved to the path as written.
    Element(syn::Path),
    /// `Button(…)`: a component, resolved to its props.
    Component(syn::Path),
}

impl Tag {
    /// Parses what a node names.
    pub(crate) fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let first = Name::parse(input)?;
        if !input.peek(Token![::]) {
            if first.text.contains('-') || !starts_upper(&first.text) {
                return Ok(Self::Intrinsic(first));
            }
            let ident = first.ident()?;
            return Ok(Self::Component(syn::parse_quote!(#ident)));
        }
        let mut path: syn::Path = {
            let ident = first.ident()?;
            syn::parse_quote!(#ident)
        };
        while input.peek(Token![::]) {
            input.parse::<Token![::]>()?;
            let segment = Name::parse(input)?;
            let ident = segment.ident()?;
            path.segments.push(syn::parse_quote!(#ident));
        }
        let last = path
            .segments
            .last()
            .expect("a path has at least one segment")
            .ident
            .to_string();
        if starts_upper(&last) {
            Ok(Self::Component(path))
        } else {
            Ok(Self::Element(path))
        }
    }

    /// Where the name was written.
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::Intrinsic(name) => name.span,
            Self::Element(path) | Self::Component(path) => path
                .segments
                .last()
                .expect("a path has at least one segment")
                .ident
                .span(),
        }
    }

    /// The name as it was written, for a diagnostic that has to repeat it.
    pub(crate) fn text(&self) -> String {
        match self {
            Self::Intrinsic(name) => name.text.clone(),
            Self::Element(path) | Self::Component(path) => path
                .to_token_stream()
                .to_string()
                .replace(' ', "")
                .to_string(),
        }
    }

    /// Whether this names a component rather than an element.
    pub(crate) fn is_component(&self) -> bool {
        matches!(self, Self::Component(_))
    }
}

/// Whether a name starts with an upper-case letter, which is what makes a node a component.
fn starts_upper(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_uppercase)
}
