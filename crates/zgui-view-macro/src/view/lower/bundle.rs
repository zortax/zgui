//! The bundle a caller forwards to a component's root element.
//!
//! A component call cannot write on an element directly — it does not know which element the
//! component will render, or how many — so everything a caller writes in the namespaced forms is
//! collected into one bundle and handed over as a prop. The component replays it on its own root.
//!
//! The order entries are written in is the order they are replayed in, which is what makes the
//! merge rules a contract: a component merges the caller's bundle *after* its own, so the caller
//! is last and therefore wins.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};

use crate::view::attr::{Attr, event, state};
use crate::view::lower::a11y::A11y;

/// The `Attrs` value a component call forwards.
#[derive(Default)]
pub(crate) struct Bundle {
    /// One call per entry, in the order the caller wrote them.
    entries: Vec<TokenStream>,
    /// The accessibility properties, which are one value however many were written.
    a11y: A11y,
    /// Whether anything at all was written.
    used: bool,
}

impl Bundle {
    /// Records one attribute.
    pub(crate) fn push(&mut self, attr: &Attr) -> syn::Result<()> {
        let span = attr.span();
        self.used = true;
        let entry = match attr {
            Attr::ClassToggle { name, value } => {
                let class = &name.text;
                quote_spanned!(span=> .class_toggle(::zgui::expansion::view::ClassName::new(#class), #value))
            }
            Attr::StyleProperty { name, value } => {
                let property = &name.text;
                quote_spanned!(span=> .style_property(#property, #value))
            }
            Attr::CustomProperty { name, value } => {
                let property = name.custom_property()?;
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
                quote_spanned!(span=> .custom_state(::zgui::expansion::view::Ident::new(#state), #value))
            }
            Attr::Listener {
                name,
                modifiers,
                value,
            } => {
                let (event, payload) = event::resolve(name)?;
                let handler = modifiers.wrap(quote!(#value), &payload, span);
                let options = modifiers.options();
                quote_spanned!(span=> .listener(#event, #options, #handler))
            }
            Attr::A11y { name, value } => {
                self.a11y.push(name, value)?;
                TokenStream::new()
            }
            Attr::Spread(value) => quote_spanned!(span=> .merged(#value)),
            Attr::StyleText(value) => {
                return Err(syn::Error::new(
                    value.span,
                    "a whole `style` attribute on a component would replace whatever the \
                     component itself set\n\n\
                     help: forward one declaration at a time: `style:gap=\"1rem\"`",
                ));
            }
            _ => unreachable!("the caller routes every other form itself"),
        };
        self.entries.push(entry);
        Ok(())
    }

    /// The bundle, or nothing when the caller forwarded nothing.
    pub(crate) fn build(self) -> syn::Result<Option<TokenStream>> {
        if !self.used {
            return Ok(None);
        }
        let entries = self.entries;
        let a11y = self
            .a11y
            .build()
            .map(|binding| quote!(.a11y_from(#binding)));
        Ok(Some(
            quote!(::zgui::expansion::view::Attrs::new() #(#entries)* #a11y),
        ))
    }
}
