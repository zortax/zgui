//! Event names, and the modifiers a listener registration takes.
//!
//! An `on:` name is snake_case and resolves to a constant whose own type carries the payload, so
//! the handler's argument type is inferred from the name alone and a misspelling is a compile
//! error rather than a listener that never fires.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Token;
use syn::parse::ParseStream;

use crate::view::attr::name::Name;

/// Every event a view can listen for.
const EVENTS: &[&str] = &[
    "animation_cancel",
    "animation_end",
    "animation_iteration",
    "animation_start",
    "change",
    "click",
    "context_menu",
    "double_click",
    "drop",
    "focus_in",
    "focus_out",
    "ime_commit",
    "ime_end",
    "ime_preedit",
    "ime_start",
    "input",
    "key_down",
    "key_up",
    "pointer_cancel",
    "pointer_down",
    "pointer_enter",
    "pointer_leave",
    "pointer_move",
    "pointer_up",
    "scroll",
    "text",
    "transition_cancel",
    "transition_end",
    "transition_run",
    "transition_start",
    "wheel",
];

/// Resolves `on:name` to the event constant it registers under, and the event's own type.
///
/// The type is named as well as the constant because a handler is wrapped when a modifier asks
/// for one, and a closure whose argument type is inferred rather than written is not
/// higher-ranked over the lifetime of the context it is handed.
pub(crate) fn resolve(name: &Name) -> syn::Result<(TokenStream, TokenStream)> {
    if EVENTS.contains(&name.text.as_str()) {
        let constant = syn::Ident::new(&name.text.to_uppercase(), name.span);
        let ty = syn::Ident::new(&to_camel(&name.text), name.span);
        return Ok((
            quote!(::zgui::expansion::view::events::#constant),
            quote!(::zgui::expansion::view::events::#ty),
        ));
    }
    Err(unknown(&name.text, name.span))
}

/// `pointer_down` becomes `PointerDown`.
fn to_camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = true;
    for character in name.chars() {
        if character == '_' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

/// The diagnostic for an event that does not exist, with the nearest name that does.
fn unknown(name: &str, span: Span) -> syn::Error {
    let mut message = format!("`on:{name}` is not an event");
    if let Some(nearest) = nearest(name) {
        message.push_str(&format!(
            "\n\nhelp: there is an event called `on:{nearest}`"
        ));
    } else {
        message.push_str(
            "\n\nnote: event names are snake_case: `on:click`, `on:pointer_down`, `on:key_down`",
        );
    }
    syn::Error::new(span, message)
}

/// The known event closest to `name`, if one is close enough to be worth suggesting.
fn nearest(name: &str) -> Option<&'static str> {
    EVENTS
        .iter()
        .map(|event| (*event, distance(name, event)))
        .filter(|(event, distance)| *distance * 3 <= event.len().max(name.len()))
        .min_by_key(|(_, distance)| *distance)
        .map(|(event, _)| event)
}

/// Levenshtein distance, for the suggestion only.
fn distance(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (row, left_char) in left.chars().enumerate() {
        current[0] = row + 1;
        for (column, right_char) in right.iter().enumerate() {
            let substitution = previous[column] + usize::from(left_char != *right_char);
            current[column + 1] = substitution
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
        }
        core::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

/// What `:capture`, `:passive`, `:once`, `:prevent` and `:stop` asked for.
#[derive(Default, Clone)]
pub(crate) struct Modifiers {
    /// Register on the way down.
    capture: bool,
    /// Promise never to cancel.
    passive: bool,
    /// Remove the listener after it runs.
    once: bool,
    /// Suppress the default behaviour before the handler runs.
    prevent: bool,
    /// Stop the event travelling before the handler runs.
    stop: bool,
}

impl Modifiers {
    /// Parses `:capture:once…`, which follows the event name.
    pub(crate) fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut modifiers = Self::default();
        while input.peek(Token![:]) && !input.peek(Token![::]) {
            input.parse::<Token![:]>()?;
            let name = Name::parse(input)?;
            match name.text.as_str() {
                "capture" => modifiers.capture = true,
                "passive" => modifiers.passive = true,
                "once" => modifiers.once = true,
                "prevent" => modifiers.prevent = true,
                "stop" => modifiers.stop = true,
                other => {
                    return Err(syn::Error::new(
                        name.span,
                        format!(
                            "`:{other}` is not a listener modifier\n\n\
                             note: the modifiers are `:capture`, `:passive`, `:once`, \
                             `:prevent` and `:stop`"
                        ),
                    ));
                }
            }
        }
        if modifiers.passive && modifiers.prevent {
            return Err(syn::Error::new(
                input.span(),
                "`:passive` promises never to suppress the default behaviour, and `:prevent` \
                 suppresses it",
            ));
        }
        Ok(modifiers)
    }

    /// Whether the registration differs from the ordinary one.
    pub(crate) fn are_registration_options(&self) -> bool {
        self.capture || self.passive || self.once
    }

    /// The options the listener is registered with.
    pub(crate) fn options(&self) -> TokenStream {
        let capture = self.capture;
        let passive = self.passive;
        let once = self.once;
        quote!(::zgui::expansion::view::ListenerOptions { capture: #capture, passive: #passive, once: #once })
    }

    /// Wraps a handler in whatever `:prevent` and `:stop` asked to happen first.
    pub(crate) fn wrap(
        &self,
        handler: TokenStream,
        event: &TokenStream,
        span: Span,
    ) -> TokenStream {
        if !self.prevent && !self.stop {
            return handler;
        }
        let prevent = self
            .prevent
            .then(|| quote!(::zgui::expansion::view::EventCx::prevent_default(event);));
        let stop = self
            .stop
            .then(|| quote!(::zgui::expansion::view::EventCx::stop_propagation(event);));
        quote::quote_spanned!(span=> {
            // The handler is bound through a function whose parameter is higher-ranked over the
            // context's lifetime, because a closure whose argument type is only ever inferred
            // from a call inside another closure is not.
            fn handler_of<E, F>(handler: F) -> F
            where
                E: ::zgui::expansion::view::EventType,
                F: ::core::ops::Fn(&mut ::zgui::expansion::view::EventCx<'_, E>) + 'static,
            {
                handler
            }
            let handler = handler_of::<#event, _>(#handler);
            move |event: &mut ::zgui::expansion::view::EventCx<'_, #event>| {
                #prevent
                #stop
                handler(event)
            }
        })
    }
}
