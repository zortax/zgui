//! `#[component]`: a function, the props it takes, and the scope it runs in.

pub(crate) mod builder;
pub(crate) mod prop;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{GenericParam, ItemFn, ReturnType};

/// Expands `#[component]`.
pub(crate) fn expand(attribute: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let slot_aware = parse_options(attribute)?;
    let mut function = syn::parse2::<ItemFn>(item)?;
    let name = function.sig.ident.clone();
    if !name
        .to_string()
        .chars()
        .next()
        .is_some_and(char::is_uppercase)
    {
        return Err(syn::Error::new(
            name.span(),
            format!(
                "a component's name is upper camel case, because a lower-case name in a view \
                 names an element\n\n\
                 help: `{name}()` is an element; a component is written `{camel}()`",
                camel = to_camel(&name.to_string())
            ),
        ));
    }
    if matches!(function.sig.output, ReturnType::Default) {
        return Err(syn::Error::new(
            function.sig.span(),
            "a component returns a view: `-> impl IntoView`",
        ));
    }
    if let Some(lifetime) = function
        .sig
        .generics
        .params
        .iter()
        .find(|parameter| matches!(parameter, GenericParam::Lifetime(_)))
    {
        return Err(syn::Error::new(
            lifetime.span(),
            "a component's props outlive the call that built them, so a component takes no \
             lifetime parameter",
        ));
    }

    let props_name = format_ident!("{name}Props");
    let props = prop::from_arguments(&function.sig.inputs)?;
    if let Some(second) = props
        .iter()
        .filter(|prop| matches!(prop.requirement, prop::Requirement::Attrs))
        .nth(1)
    {
        return Err(syn::Error::new(
            second.span,
            "a component forwards one bundle, so exactly one prop is `#[prop(attrs)]`",
        ));
    }
    let docs = format!("The props of [`{name}`].");
    let generated = builder::generate(
        &function.vis,
        &name,
        &props_name,
        &function.sig.generics,
        &quote!(#[doc = #docs]),
        &props,
        None,
    )?;

    strip_prop_attributes(&mut function);
    let generics = &function.sig.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let turbofish = ty_generics.as_turbofish();
    let fields: Vec<&syn::Ident> = props.iter().map(|prop| &prop.field).collect();
    let identity = format!("The module path and name of [`{name}`], for tooling.");
    let meta_doc = format!(
        "Where [`{name}`] was declared, for tooling.\n\n\
         Read by the inspector's component tree, which shows each instance against the file and \
         line its component was written on."
    );
    let slots_doc = format!("Whether [`{name}`] takes slot children.");
    let render_doc = format!(
        "Builds [`{name}`] with these props, in a reactive scope of its own.\n\n\
         Everything the component allocates is freed the moment its view is unmounted."
    );

    Ok(quote! {
        #[allow(non_snake_case)]
        #function

        #generated

        #[allow(unreachable_pub)]
        impl #impl_generics #props_name #ty_generics #where_clause {
            #[doc = #identity]
            pub const COMPONENT_ID: &'static str =
                ::core::concat!(::core::module_path!(), "::", ::core::stringify!(#name));

            #[doc = #meta_doc]
            pub const COMPONENT_META: ::zgui::expansion::view::ComponentMeta =
                ::zgui::expansion::view::ComponentMeta {
                    name: Self::COMPONENT_ID,
                    file: ::core::file!(),
                    line: ::core::line!(),
                };

            #[doc = #slots_doc]
            #[doc(hidden)]
            pub const ACCEPTS_SLOTS: bool = #slot_aware;

            #[doc = #render_doc]
            #[cfg_attr(debug_assertions, track_caller)]
            pub fn render(self) -> impl ::zgui::expansion::view::IntoView {
                ::zgui::expansion::view::Scoped::named(
                    &Self::COMPONENT_META,
                    move || #name #turbofish (#(self.#fields),*),
                )
            }
        }
    })
}

/// Reads `#[component(slot_aware)]`.
fn parse_options(attribute: TokenStream) -> syn::Result<bool> {
    if attribute.is_empty() {
        return Ok(false);
    }
    let ident = syn::parse2::<syn::Ident>(attribute)?;
    if ident != "slot_aware" {
        return Err(syn::Error::new(
            ident.span(),
            "`slot_aware` is the only option a component takes",
        ));
    }
    Ok(true)
}

/// Removes the attributes only this macro understands, which rustc would reject on a parameter.
fn strip_prop_attributes(function: &mut ItemFn) {
    for input in &mut function.sig.inputs {
        if let syn::FnArg::Typed(typed) = input {
            typed.attrs.clear();
        }
    }
}

/// `alert` becomes `Alert`.
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
