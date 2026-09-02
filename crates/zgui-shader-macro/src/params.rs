//! The `ShaderParams` derive: a structure's layout, read out of the compiler.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, spanned::Spanned};

/// Expands the derive.
pub(crate) fn expand(input: TokenStream) -> Result<TokenStream, syn::Error> {
    let input: DeriveInput = syn::parse2(input)?;
    let name = &input.ident;
    let fields = named_fields(&input)?;

    if !is_repr_c(&input) {
        return Err(syn::Error::new(
            input.ident.span(),
            "a parameter structure is compared against a shader's declaration field by field, so \
             it must be `#[repr(C)]`",
        ));
    }

    // A field's width is where the next one begins, and the last one's is where the structure
    // ends. That reads a field's size out of the compiler without evaluating a value, which a
    // constant cannot do. Where the two disagree — a structure with padding — the comparison
    // against the shader's own declaration is what catches it.
    let entries = fields.iter().enumerate().map(|(index, field)| {
        let ident = field.ident.as_ref().expect("named fields were checked");
        let text = ident.to_string();
        let end = match fields.get(index + 1) {
            Some(next) => {
                let next = next.ident.as_ref().expect("named fields were checked");
                quote! { ::core::mem::offset_of!(#name, #next) }
            }
            None => quote! { ::core::mem::size_of::<#name>() },
        };
        quote! {
            ::zgui::shader::ParamsField {
                name: #text,
                offset: ::core::mem::offset_of!(#name, #ident),
                size: #end - ::core::mem::offset_of!(#name, #ident),
            }
        }
    });

    let writes = fields.iter().map(|field| {
        let ident = field.ident.as_ref().expect("named fields were checked");
        let ty = &field.ty;
        quote! {
            {
                let offset = ::core::mem::offset_of!(#name, #ident);
                let width = <#ty as ::zgui::shader::ParamsValue>::BYTES;
                ::zgui::shader::ParamsValue::write(
                    self.#ident,
                    &mut out[offset..offset + width],
                );
            }
        }
    });

    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        // Every offset below comes from the compiler rather than from anything written by hand,
        // so the layout cannot drift from the structure it describes.
        impl #impl_generics ::zgui::shader::ShaderParams for #name #type_generics #where_clause {
            const LAYOUT: ::zgui::shader::ParamsLayout = ::zgui::shader::ParamsLayout {
                size: ::core::mem::size_of::<#name>(),
                fields: &[#(#entries),*],
            };

            fn write(&self, out: &mut [u8; ::zgui::shader::MAX_PARAMS_BYTES]) {
                #(#writes)*
            }
        }
    })
}

/// The structure's named fields, or an error naming what it was instead.
fn named_fields(input: &DeriveInput) -> Result<Vec<syn::Field>, syn::Error> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.ident.span(),
            "shader parameters are a struct: an enum or a union has no field layout to compare",
        ));
    };
    match &data.fields {
        Fields::Named(named) => Ok(named.named.iter().cloned().collect()),
        Fields::Unit => Ok(Vec::new()),
        Fields::Unnamed(unnamed) => Err(syn::Error::new(
            unnamed.span(),
            "shader parameters are compared against a shader's declaration by name, so the fields \
             must be named",
        )),
    }
}

/// Whether the structure carries `#[repr(C)]`.
fn is_repr_c(input: &DeriveInput) -> bool {
    input.attrs.iter().any(|attr| {
        if !attr.path().is_ident("repr") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("C") {
                found = true;
            }
            Ok(())
        });
        found
    })
}
