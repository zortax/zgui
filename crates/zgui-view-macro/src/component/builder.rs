//! The props struct and its typestate builder.
//!
//! A prop that must be given contributes a type parameter to the builder, flipped from unset to
//! set by its own setter. `build` requires every one of them to be set, through a trait per prop
//! whose unimplemented message names the component and the prop — which is what turns a missing
//! required prop from a run-time panic into a compile error that says which prop is missing.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{GenericParam, Generics, Ident, Visibility};

use crate::component::prop::Prop;

/// Generates the props struct, its builder and the markers that gate `build`.
pub(crate) fn generate(
    visibility: &Visibility,
    subject: &Ident,
    props: &Ident,
    generics: &Generics,
    props_docs: &TokenStream,
    fields: &[Prop],
    field_visibility: Option<&[Visibility]>,
) -> syn::Result<TokenStream> {
    let markers = format_ident!("__{}_props", snake_case(&subject.to_string()));
    let builder = format_ident!("{props}Builder");
    let where_clause = &generics.where_clause;

    let declared: Vec<TokenStream> = generics
        .params
        .iter()
        .map(|parameter| quote!(#parameter))
        .collect();
    let arguments: Vec<TokenStream> = generics.params.iter().map(argument_of).collect();

    let required: Vec<&Prop> = fields.iter().filter(|prop| prop.is_required()).collect();
    let parameters: Vec<Ident> = required
        .iter()
        .map(|prop| format_ident!("__Prop{}", to_camel(&prop.field.to_string())))
        .collect();
    let traits: Vec<Ident> = required
        .iter()
        .map(|prop| format_ident!("Has{}", to_camel(&prop.field.to_string())))
        .collect();

    let props_declared = angled(&declared);
    let props_used = angled(&arguments);
    let builder_declared = angled(
        &declared
            .iter()
            .cloned()
            .chain(
                parameters
                    .iter()
                    .map(|parameter| quote!(#parameter = #markers::Unset)),
            )
            .collect::<Vec<_>>(),
    );
    let builder_impl = angled(
        &declared
            .iter()
            .cloned()
            .chain(parameters.iter().map(|parameter| quote!(#parameter)))
            .collect::<Vec<_>>(),
    );
    let builder_used = angled(
        &arguments
            .iter()
            .cloned()
            .chain(parameters.iter().map(|parameter| quote!(#parameter)))
            .collect::<Vec<_>>(),
    );

    let marker_traits = required.iter().zip(&traits).map(|(prop, name)| {
        let message = format!("`{subject}` is missing the required prop `{}`", prop.setter);
        let label = format!("`{}` was never given", prop.setter);
        let docs = format!("Satisfied once the `{}` prop has been given.", prop.setter);
        quote! {
            #[doc = #docs]
            #[diagnostic::on_unimplemented(message = #message, label = #label)]
            pub trait #name {}
            impl #name for Set {}
        }
    });

    let struct_fields = fields.iter().enumerate().map(|(index, prop)| {
        let docs = &prop.docs;
        let field = &prop.field;
        let ty = &prop.ty;
        let visibility = field_visibility.map(|visibilities| &visibilities[index]);
        quote!(#(#docs)* #visibility #field: #ty,)
    });

    let builder_fields = fields.iter().map(|prop| {
        let field = &prop.field;
        let ty = &prop.ty;
        quote!(#field: ::core::option::Option<#ty>,)
    });

    let empty = fields.iter().map(|prop| {
        let field = &prop.field;
        quote!(#field: ::core::option::Option::None,)
    });

    let setters = fields.iter().map(|prop| {
        let docs = &prop.docs;
        let field = &prop.field;
        let setter = &prop.setter;
        let (parameter, stored) = prop.setter_signature();
        let extra = prop.setter_generics();
        let carried = fields
            .iter()
            .filter(|other| other.field != prop.field)
            .map(|other| {
                let other = &other.field;
                quote!(#other: self.#other,)
            });
        let returned = angled(
            &arguments
                .iter()
                .cloned()
                .chain(parameters.iter().zip(&required).map(|(marker, required)| {
                    if required.field == prop.field {
                        quote!(#markers::Set)
                    } else {
                        quote!(#marker)
                    }
                }))
                .collect::<Vec<_>>(),
        );
        quote::quote_spanned! {prop.span=>
            #(#docs)*
            pub fn #setter<#extra>(self, #parameter) -> #builder #returned {
                #builder {
                    #field: ::core::option::Option::Some(#stored),
                    #(#carried)*
                    __markers: ::core::marker::PhantomData,
                }
            }
        }
    });

    let built = fields.iter().map(|prop| {
        let field = &prop.field;
        let fallback = prop.fallback();
        quote!(#field: { let value = self.#field; #fallback },)
    });

    let bounds = parameters
        .iter()
        .zip(&traits)
        .map(|(parameter, name)| quote!(#parameter: #markers::#name));

    let builder_docs = format!("Builds the props of [`{subject}`].");
    let markers_docs = format!("The typestate of [`{builder}`], and the markers behind it.");
    let phantom = quote!(::core::marker::PhantomData<(#(#parameters,)*)>);

    Ok(quote! {
        #props_docs
        #[allow(unreachable_pub)]
        #visibility struct #props #props_declared #where_clause {
            #(#struct_fields)*
        }

        #[doc = #markers_docs]
        #[doc(hidden)]
        #[allow(unreachable_pub)]
        #visibility mod #markers {
            /// A prop that has been given.
            pub struct Set;
            /// A prop that has not been given.
            pub struct Unset;
            #(#marker_traits)*
        }

        #[doc = #builder_docs]
        #[allow(unreachable_pub)]
        #visibility struct #builder #builder_declared #where_clause {
            #(#builder_fields)*
            __markers: #phantom,
        }

        #[allow(unreachable_pub)]
        impl #props_declared #props #props_used #where_clause {
            #[doc = #builder_docs]
            pub fn builder() -> #builder #props_used {
                #builder {
                    #(#empty)*
                    __markers: ::core::marker::PhantomData,
                }
            }
        }

        #[allow(unreachable_pub)]
        impl #builder_impl #builder #builder_used #where_clause {
            #(#setters)*

            /// Finishes the props, once every required prop has been given.
            pub fn build(self) -> #props #props_used
            where
                #(#bounds,)*
            {
                #props { #(#built)* }
            }
        }
    })
}

/// A generic parameter as an argument: its name, without its bounds.
fn argument_of(parameter: &GenericParam) -> TokenStream {
    match parameter {
        GenericParam::Type(parameter) => {
            let ident = &parameter.ident;
            quote!(#ident)
        }
        GenericParam::Lifetime(parameter) => {
            let lifetime = &parameter.lifetime;
            quote!(#lifetime)
        }
        GenericParam::Const(parameter) => {
            let ident = &parameter.ident;
            quote!(#ident)
        }
    }
}

/// Wraps a parameter list in angle brackets, or writes nothing when it is empty.
fn angled(parts: &[TokenStream]) -> TokenStream {
    if parts.is_empty() {
        return TokenStream::new();
    }
    quote!(<#(#parts),*>)
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

/// `card_header` becomes `CardHeader`.
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
