//! A component call, and the bundle a caller forwards to it.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};

use crate::view::attr::Attr;
use crate::view::lower::bundle::Bundle;
use crate::view::lower::children;
use crate::view::node::{Node, Tagged};
use crate::view::tag::Tag;

/// Lowers a component call to its props, built and rendered.
pub(crate) fn lower(node: &Tagged) -> syn::Result<TokenStream> {
    let Tag::Component(path) = &node.tag else {
        unreachable!("an element is lowered as an element")
    };
    let props = props_path(path);
    let mut setters = TokenStream::new();
    let mut bundle = Bundle::default();
    let mut arguments: Vec<syn::Ident> = Vec::new();
    let mut classes = Vec::new();
    for attr in &node.attrs {
        let span = attr.span();
        match attr {
            Attr::Named { name, value } => {
                let method = name.ident()?;
                setters.extend(quote_spanned!(span=> .#method(#value)));
            }
            // Collected rather than lowered one by one. A props builder's setter replaces what an
            // earlier call stored, so `class = A, …, class = B` lowered as two calls silently
            // drops A — and that spelling is how every wrapper component adds its own class while
            // still taking the caller's. An element merges its class attributes; a component call
            // has to mean the same thing.
            Attr::Class(value) => classes.push((value, span)),
            Attr::Let(ident) => arguments.push(ident.clone()),
            Attr::Slot { span, .. } => {
                return Err(syn::Error::new(
                    *span,
                    "`slot` belongs on a component written inside another component's tags",
                ));
            }
            // `node_ref` on a component is an ordinary prop of that component's, not the
            // element attribute of the same name: a component renders whatever it likes and only
            // it knows which of its elements a caller means. Lowering it as a setter is what makes
            // the common case — a label and the control it names — writable at all, and a
            // component that takes no such prop gets an error naming the prop rather than one
            // about a spelling of `node_ref` that was never wrong.
            Attr::NodeRef(value) => {
                setters.extend(quote_spanned!(value.span=> .node_ref(#value)));
            }
            other => bundle.push(other)?,
        }
    }
    if let Some(((first, first_span), rest)) = classes.split_first() {
        if rest.is_empty() {
            setters.extend(quote_spanned!(*first_span=> .class(#first)));
        } else {
            let merged = rest.iter().map(|(value, span)| {
                quote_spanned!(*span=> .merged(&::zgui::expansion::view::Classes::from(#value)))
            });
            setters.extend(quote_spanned!(*first_span=>
                .class(::zgui::expansion::view::Classes::from(#first) #(#merged)*)
            ));
        }
    }
    let (slots, children) = split_slots(&node.children);
    for slot in &slots {
        setters.extend(lower_slot(slot)?);
    }
    if let Some(attrs) = bundle.build()? {
        setters.extend(quote!(.attrs(#attrs)));
    }
    if children.is_empty() {
        if let Some(argument) = arguments.first() {
            return Err(syn::Error::new(
                argument.span(),
                "`let:` names an argument for children that are not there",
            ));
        }
    } else {
        let view = children::lower_all(&children)?;
        setters.extend(quote_spanned!(node.span=>
            .children(move |#(#arguments),*| ::zgui::expansion::view::AnyView::new(#view))
        ));
    }
    let assertion = (!slots.is_empty()).then(|| {
        let message = format!(
            "`{}` is given a slot child, so it needs `#[component(slot_aware)]`",
            node.tag.text()
        );
        quote_spanned!(node.span=> const _: () = ::core::assert!(#props::ACCEPTS_SLOTS, #message);)
    });
    Ok(quote_spanned!(node.span=> {
        #assertion
        #props::builder() #setters .build().render()
    }))
}

/// The props struct of a component: its own path with `Props` on the end.
fn props_path(path: &syn::Path) -> syn::Path {
    let mut props = path.clone();
    let last = props
        .segments
        .last_mut()
        .expect("a path has a last segment");
    last.ident = syn::Ident::new(&format!("{}Props", last.ident), last.ident.span());
    props
}

/// Splits slot children out of the ordinary ones.
fn split_slots(children: &[Node]) -> (Vec<&Tagged>, Vec<&Node>) {
    let mut slots = Vec::new();
    let mut rest = Vec::new();
    for child in children {
        match child {
            Node::Tagged(tagged) if slot_name(tagged).is_some() => slots.push(tagged),
            other => rest.push(other),
        }
    }
    (slots, rest)
}

/// The prop a slot child fills, when it is one.
fn slot_name(node: &Tagged) -> Option<String> {
    node.attrs.iter().find_map(|attr| match attr {
        Attr::Slot { name, .. } => Some(
            name.clone()
                .unwrap_or_else(|| snake_case(&last_segment(&node.tag))),
        ),
        _ => None,
    })
}

/// Lowers one slot child to the setter it fills.
fn lower_slot(node: &Tagged) -> syn::Result<TokenStream> {
    let Tag::Component(path) = &node.tag else {
        return Err(syn::Error::new(
            node.span,
            "a slot is a `#[slot]` type, and an element is not one",
        ));
    };
    let value = lower_slot_value(node, path)?;
    let setter = syn::Ident::new(&slot_name(node).expect("this node is a slot"), node.span);
    Ok(quote_spanned!(node.span=> .#setter(#value)))
}

/// Builds a slot's own value: its builder, its props and its children.
fn lower_slot_value(node: &Tagged, path: &syn::Path) -> syn::Result<TokenStream> {
    let mut setters = TokenStream::new();
    let mut arguments: Vec<syn::Ident> = Vec::new();
    for attr in &node.attrs {
        let span = attr.span();
        match attr {
            Attr::Named { name, value } => {
                let method = name.ident()?;
                setters.extend(quote_spanned!(span=> .#method(#value)));
            }
            // A slot is a props struct like any other, so a whole class list is its `class` prop
            // — the same routing a component call gets, for the same reason.
            Attr::Class(value) => setters.extend(quote_spanned!(span=> .class(#value))),
            Attr::Let(ident) => arguments.push(ident.clone()),
            Attr::Slot { .. } => {}
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "a slot takes props of its own, and nothing that belongs to an element\n\n\
                     note: a slot is not an element, so it has nowhere to put a listener, an \
                     attribute or a state\n\
                     help: put them on an element inside the slot's children",
                ));
            }
        }
    }
    // A slot's props are props, so a slot of its own is one of them: `<DialogContent slot="content">`
    // holding a `<DialogTitle slot="title">` fills `DialogContent`'s `title`, exactly as a slot
    // child of a component fills the component's.
    let (slots, children) = split_slots(&node.children);
    for slot in &slots {
        setters.extend(lower_slot(slot)?);
    }
    if !children.is_empty() {
        let view = children::lower_all(&children)?;
        setters.extend(quote_spanned!(node.span=>
            .children(move |#(#arguments),*| ::zgui::expansion::view::AnyView::new(#view))
        ));
    }
    Ok(quote_spanned!(node.span=> #path::builder() #setters .build()))
}

/// The last segment of a tag, for naming the prop a slot fills.
fn last_segment(tag: &Tag) -> String {
    match tag {
        Tag::Component(path) | Tag::Element(path) => path
            .segments
            .last()
            .expect("a path has a last segment")
            .ident
            .to_string(),
        Tag::Intrinsic(name) => name.text.clone(),
    }
}

/// `CardHeader` becomes `card_header`.
fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() {
            if index != 0 {
                out.push('_');
            }
            out.extend(character.to_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}
