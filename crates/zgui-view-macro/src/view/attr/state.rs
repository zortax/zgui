//! The states a view may assert about its own element.
//!
//! Every other interaction state is computed from what actually happened — a pointer arrived, a
//! key moved focus — and a view that could assert one would be lying to the system that maintains
//! it. Author-defined states go through `custom_state:`, which is expressible on any backend.

use proc_macro2::Span;
use quote::quote;

use crate::view::attr::name::Name;

/// Each state a view may set, paired with the constant it lowers to.
const STATES: &[(&str, &str)] = &[
    ("checked", "CHECKED"),
    ("disabled", "DISABLED"),
    ("indeterminate", "INDETERMINATE"),
    ("invalid", "INVALID"),
    ("open", "OPEN"),
    ("placeholder_shown", "PLACEHOLDER_SHOWN"),
    ("read_only", "READ_ONLY"),
    ("required", "REQUIRED"),
];

/// Resolves `state:name` to the constant it sets.
pub(crate) fn resolve(name: &Name) -> syn::Result<proc_macro2::TokenStream> {
    if let Some((_, constant)) = STATES.iter().find(|(state, _)| *state == name.text) {
        let constant = syn::Ident::new(constant, name.span);
        return Ok(quote!(::zgui::expansion::view::UiState::#constant));
    }
    Err(unknown(&name.text, name.span))
}

/// The diagnostic for a state a view may not assert.
fn unknown(name: &str, span: Span) -> syn::Error {
    let known = STATES
        .iter()
        .map(|(state, _)| *state)
        .collect::<Vec<_>>()
        .join("`, `");
    let mut message = format!(
        "`state:{name}` is not one of the states a view may set\n\n\
         note: the states a view may set are `{known}`\n\
         help: an author-defined state is written `custom_state:{name}=…`, and matches \
         `:state({name})` in CSS"
    );
    if matches!(
        name,
        "hover" | "active" | "focus" | "focus_visible" | "focus_within"
    ) {
        message.push_str(&format!(
            "\nnote: `{name}` is computed by the input system from what the pointer and the \
             keyboard did, so a view cannot assert it"
        ));
    }
    if name == "selected" {
        message.push_str(
            "\nnote: no selector expresses selection, which is why it is a custom state rather \
             than one of the eight",
        );
    }
    syn::Error::new(span, message)
}
