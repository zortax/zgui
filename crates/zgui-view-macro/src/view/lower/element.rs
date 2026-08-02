//! An element's builder chain.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};

use crate::view::attr::{Attr, event, state};
use crate::view::lower::a11y::A11y;
use crate::view::lower::children;
use crate::view::node::Tagged;
use crate::view::tag::Tag;

/// Lowers an element to `builder().attribute(…)….child(…)`.
pub(crate) fn lower(node: &Tagged) -> syn::Result<TokenStream> {
    let mut expanded = builder(&node.tag);
    let mut a11y = A11y::default();
    for attr in &node.attrs {
        let span = attr.span();
        let call = match attr {
            Attr::Named { name, value } => {
                let method = name.ident()?;
                quote_spanned!(span=> .#method(#value))
            }
            Attr::Class(value) => quote_spanned!(span=> .class(#value)),
            Attr::ClassToggle { name, value } => {
                let class = &name.text;
                quote_spanned!(span=> .class_toggle(::zgui::expansion::view::ClassName::new(#class), #value))
            }
            Attr::StyleText(value) => quote_spanned!(span=> .style_text(#value)),
            Attr::StyleProperty { name, value } => {
                let property = &name.text;
                quote_spanned!(span=> .style_property(#property, #value))
            }
            Attr::CustomProperty { name, value } => {
                let property = custom_property_name(name)?;
                quote_spanned!(span=>
                    .custom_property(::zgui::expansion::view::CustomPropertyName::new(#property), #value)
                )
            }
            Attr::Attribute { name, value } => {
                let attribute = &name.text;
                quote_spanned!(span=> .attribute(::zgui::expansion::view::AttrName::new(#attribute), #value))
            }
            Attr::Property { name, value } => {
                let property = &name.text;
                quote_spanned!(span=> .property(::zgui::expansion::view::PropKey::new(#property), #value))
            }
            Attr::State { name, value } => {
                let state = state::resolve(name)?;
                quote_spanned!(span=> .state(#state, #value))
            }
            Attr::CustomState { name, value } => {
                let state = &name.text;
                quote_spanned!(span=>
                    .custom_state(::zgui::expansion::view::Ident::new(#state), #value)
                )
            }
            Attr::Listener {
                name,
                modifiers,
                value,
            } => {
                let (event, payload) = event::resolve(name)?;
                let handler = modifiers.wrap(quote!(#value), &payload, span);
                if modifiers.are_registration_options() {
                    let options = modifiers.options();
                    quote_spanned!(span=> .on_with(#event, #options, #handler))
                } else {
                    quote_spanned!(span=> .on(#event, #handler))
                }
            }
            Attr::A11y { name, value } => {
                a11y.push(name, value)?;
                TokenStream::new()
            }
            Attr::NodeRef(value) => quote_spanned!(span=> .node_ref(#value)),
            Attr::Spread(value) => quote_spanned!(span=> .attrs(#value)),
            Attr::Let(ident) => {
                return Err(syn::Error::new(
                    ident.span(),
                    "`let:` names an argument a component passes to its children, and an element \
                     passes none",
                ));
            }
            Attr::Slot { span, .. } => {
                return Err(syn::Error::new(
                    *span,
                    "`slot` marks a component as filling a slot of its parent, and an element \
                     cannot fill one",
                ));
            }
        };
        expanded.extend(call);
    }
    if let Some(binding) = a11y.build() {
        expanded.extend(quote!(.a11y(#binding)));
    }
    for child in &node.children {
        let child = children::lower(child)?;
        expanded.extend(quote!(.child(#child)));
    }
    Ok(expanded)
}

/// The call that starts an element's chain.
fn builder(tag: &Tag) -> TokenStream {
    match tag {
        Tag::Intrinsic(name) => {
            let function = intrinsic_ident(&name.text.replace('-', "_"), name.span);
            quote_spanned!(name.span=> ::zgui::expansion::elements::#function())
        }
        Tag::Element(path) => quote_spanned!(tag.span()=> #path()),
        Tag::Component(_) => unreachable!("a component is lowered as a component"),
    }
}

/// The identifier that calls an intrinsic element's builder.
///
/// Written as a raw identifier when the element's name is also a Rust keyword. `<box/>` is the
/// commonest element there is and `box` is a reserved word, so an ordinary identifier here expands
/// to a call on `box()` — which is not a call, is not an expression, and produces a parse error at
/// the call site pointing at the user's own view.
fn intrinsic_ident(name: &str, span: proc_macro2::Span) -> syn::Ident {
    match syn::parse_str::<syn::Ident>(name) {
        Ok(_) => syn::Ident::new(name, span),
        Err(_) => syn::Ident::new_raw(name, span),
    }
}

/// The name of a custom property, in the form the name table stores.
///
/// An author writes the declaration — `var:--brand=…` — and the table keys on the name without its
/// `--`, so the prefix is dropped here. Carrying it through would intern `--brand` under the name
/// `--brand`, whose declaration is `----brand`, and a sheet saying `var(--brand)` would never find
/// what the view had written.
fn custom_property_name(name: &crate::view::attr::name::Name) -> syn::Result<String> {
    let Some(stored) = name.text.strip_prefix("--").filter(|rest| !rest.is_empty()) else {
        return Err(syn::Error::new(
            name.span,
            format!(
                "a custom property's name starts with `--`\n\n\
                 help: write `var:--{}=…`",
                name.text
            ),
        ));
    };
    Ok(stored.to_owned())
}
