//! `css!` and `style!`: CSS checked where it is written.

mod scope;
mod validate;

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token, Visibility};

/// One or more string literals, which are joined with a newline between them.
struct Rules {
    /// The joined text.
    text: String,
    /// The first literal, which is what a diagnostic points at.
    span: proc_macro2::Span,
}

impl Parse for Rules {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let first = input.parse::<LitStr>()?;
        let span = first.span();
        let mut text = first.value();
        while !input.is_empty() {
            let _ = input.parse::<Option<Token![,]>>()?;
            if input.is_empty() {
                break;
            }
            let next = input.parse::<LitStr>()?;
            text.push('\n');
            text.push_str(&next.value());
        }
        Ok(Self { text, span })
    }
}

/// Expands `css!`.
pub(crate) fn expand_css(input: TokenStream) -> syn::Result<TokenStream> {
    let rules = syn::parse2::<Rules>(input)?;
    validate::validate(&rules.text, rules.span)?;
    let text = &rules.text;
    Ok(quote!(#text))
}

/// A `style!` invocation: a visibility, a name and the rules.
struct Sheet {
    /// How visible the generated type is.
    visibility: Visibility,
    /// What the sheet is called.
    name: Ident,
    /// The rules.
    rules: Rules,
}

impl Parse for Sheet {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let visibility = input.parse::<Visibility>()?;
        let name = input.parse::<Ident>()?;
        input.parse::<Token![=>]>().map_err(|_| {
            syn::Error::new(
                name.span(),
                format!("a scoped sheet is written `style! {{ {name} => \"…\" }}`"),
            )
        })?;
        let rules = input.parse::<Rules>()?;
        Ok(Self {
            visibility,
            name,
            rules,
        })
    }
}

/// Expands `style!`.
pub(crate) fn expand_style(input: TokenStream) -> syn::Result<TokenStream> {
    let sheet = syn::parse2::<Sheet>(input)?;
    validate::validate(&sheet.rules.text, sheet.rules.span)?;
    let name = &sheet.name;
    let visibility = &sheet.visibility;
    let class = scope::class_of(&name.to_string(), &sheet.rules.text);
    let css = scope::rewrite(&sheet.rules.text, &class);
    let docs = format!("The scoped stylesheet of `{name}`.");
    let class_docs = format!(
        "The class `{name}`'s rules are written against.\n\n\
         It is what the component puts on its own root element, and it is derived from the name \
         and the text of the rules, so it is stable across builds and collides with nothing."
    );
    let css_docs = format!(
        "The rules of `{name}`, with `:scope` already resolved to [`{name}::CLASS`].\n\n\
         The text is ready to be registered as an author-origin stylesheet."
    );
    Ok(quote! {
        #[doc = #docs]
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
        #visibility struct #name {}

        impl #name {
            #[doc = #class_docs]
            pub const CLASS: &'static str = #class;

            #[doc = #css_docs]
            pub const CSS: &'static str = #css;
        }
    })
}
