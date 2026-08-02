//! The `a11y:` accumulator.
//!
//! Accessibility properties are collected across the whole opening tag and lowered as one
//! binding, because a role and the properties that qualify it are one value rather than several.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};

use crate::view::attr::name::Name;
use crate::view::value::Value;

/// The `a11y:` properties written on one tag.
#[derive(Default)]
pub(crate) struct A11y {
    /// What `a11y:role` said, if it was written.
    role: Option<Value>,
    /// Every other property, in the order it was written.
    steps: Vec<TokenStream>,
}

impl A11y {
    /// Records one `a11y:` property.
    pub(crate) fn push(&mut self, name: &Name, value: &Value) -> syn::Result<()> {
        if name.text == "role" {
            if self.role.is_some() {
                return Err(syn::Error::new(name.span, "`a11y:role` is written once"));
            }
            self.role = Some(value.clone());
            return Ok(());
        }
        let method = name.ident()?;
        self.steps
            .push(quote_spanned!(name.span=> .#method(#value)));
        Ok(())
    }

    /// The binding, or nothing when the tag carried no accessibility properties.
    pub(crate) fn build(self) -> Option<TokenStream> {
        if self.role.is_none() && self.steps.is_empty() {
            return None;
        }
        let steps = self.steps;
        // A tag that named no role says nothing about what the element is, which is not the same
        // as saying it is a box: on a component call the binding is merged over the component's
        // own, and a role invented here would silently replace it.
        let start = match self.role {
            Some(role) => {
                quote_spanned!(role.span=> ::zgui::expansion::view::A11yBinding::with_role(#role))
            }
            None => quote!(::zgui::expansion::view::A11yBinding::unspecified()),
        };
        Some(quote!(#start #(#steps)*))
    }
}
