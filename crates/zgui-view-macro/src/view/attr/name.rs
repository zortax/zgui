//! Attribute names, which are not always Rust identifiers.
//!
//! `data-testid`, `margin-left` and `--brand` are all names a view writes, and none of them is an
//! identifier. A name is therefore parsed from its tokens rather than from a single `Ident`.

use proc_macro2::Span;
use syn::Token;
use syn::ext::IdentExt;
use syn::parse::ParseStream;

/// One attribute name, as written.
#[derive(Clone)]
pub(crate) struct Name {
    /// The name itself, with its dashes intact.
    pub(crate) text: String,
    /// Where it was written.
    pub(crate) span: Span,
}

impl Name {
    /// Parses `ident`, `ident-ident-…` or `--custom-property`.
    ///
    /// A `-` is part of the name when what follows it is another piece of one.
    pub(crate) fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let span = input.span();
        let mut text = String::new();
        while input.peek(Token![-]) {
            input.parse::<Token![-]>()?;
            text.push('-');
        }
        loop {
            if input.peek(syn::Ident::peek_any) {
                text.push_str(&syn::Ident::parse_any(input)?.to_string());
            } else if input.peek(syn::LitInt) {
                text.push_str(input.parse::<syn::LitInt>()?.base10_digits());
            } else if text.is_empty() {
                return Err(syn::Error::new(span, "expected an attribute name"));
            } else {
                return Err(syn::Error::new(
                    input.span(),
                    "expected a name after `-`, as in `data-testid`",
                ));
            }
            if input.peek(Token![-]) {
                input.parse::<Token![-]>()?;
                text.push('-');
                continue;
            }
            break;
        }
        Ok(Self { text, span })
    }

    /// The name as a Rust identifier, for the forms that name a method or a prop.
    pub(crate) fn ident(&self) -> syn::Result<syn::Ident> {
        if self.text.contains('-') {
            return Err(syn::Error::new(
                self.span,
                format!(
                    "`{}` is a name, not an identifier, so it cannot name a property\n\n\
                     help: an arbitrary attribute is written `attr:{}=…`",
                    self.text, self.text
                ),
            ));
        }
        Ok(match syn::parse_str::<syn::Ident>(&self.text) {
            Ok(ident) => syn::Ident::new(&ident.to_string(), self.span),
            Err(_) => syn::Ident::new_raw(&self.text, self.span),
        })
    }

    /// The name of a custom property, in the form the name table stores.
    ///
    /// An author writes the declaration — `var:--brand=…` — and the table keys on the name without
    /// its `--`, so the prefix is dropped here. Carrying it through would intern `--brand` under
    /// the name `--brand`, whose declaration is `----brand`, and a sheet saying `var(--brand)`
    /// would never find what the view had written.
    ///
    /// This lives beside [`Name::ident`] rather than in either lowering because an element and a
    /// component call both reach it: an element writes the property itself, a component call packs
    /// it into the bundle it forwards, and the two have to agree on the stored spelling for a
    /// forwarded property to name the same thing as a written one.
    pub(crate) fn custom_property(&self) -> syn::Result<&str> {
        self.text
            .strip_prefix("--")
            .filter(|rest| !rest.is_empty())
            .ok_or_else(|| {
                syn::Error::new(
                    self.span,
                    format!(
                        "a custom property's name starts with `--`\n\n\
                         help: write `var:--{}=…`",
                        self.text
                    ),
                )
            })
    }
}
