//! What a `variants!` table generates.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

use crate::variants::table::Table;

/// Generates the enumerations, the table struct and the two lowerings.
pub(crate) fn emit(table: &Table) -> TokenStream {
    let Table {
        docs,
        visibility,
        name,
        base,
        axes,
    } = table;
    let prefix = prefix_of(&name.to_string());
    let enums: Vec<Ident> = axes
        .iter()
        .map(|axis| format_ident!("{prefix}{}", to_camel(&axis.field.to_string())))
        .collect();

    let declarations = axes.iter().zip(&enums).map(|(axis, enumeration)| {
        let variants = axis.choices.iter().map(|choice| {
            let name = &choice.name;
            let class = &choice.class;
            let docs = if choice.class.value().is_empty() {
                format!("`{name}`, which adds no class of its own.")
            } else {
                format!("`{name}`, which is `{}` in CSS.", class.value())
            };
            quote! {
                #[doc = #docs]
                #name,
            }
        });
        let classes = axis.choices.iter().map(|choice| {
            let name = &choice.name;
            let class = &choice.class;
            quote!(Self::#name => #class,)
        });
        let names = axis.choices.iter().map(|choice| {
            let name = &choice.name;
            let written = kebab_case(&name.to_string());
            quote!(Self::#name => #written,)
        });
        let default = &axis.default;
        let docs = format!("How `{}` varies.", axis.field);
        quote! {
            #[doc = #docs]
            #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
            #visibility enum #enumeration {
                #(#variants)*
            }

            impl #enumeration {
                /// The class this choice adds, which is empty when it adds none.
                pub const fn class(self) -> &'static str {
                    match self { #(#classes)* }
                }

                /// What this choice is called in a `data-` attribute.
                pub const fn name(self) -> &'static str {
                    match self { #(#names)* }
                }
            }

            impl ::core::default::Default for #enumeration {
                fn default() -> Self {
                    Self::#default
                }
            }
        }
    });

    let fields = axes.iter().zip(&enums).map(|(axis, enumeration)| {
        let field = &axis.field;
        let docs = format!("Which `{field}` this is.");
        quote! {
            #[doc = #docs]
            pub #field: #enumeration,
        }
    });
    let base_class = match base {
        Some(base) => quote!(#base),
        None => quote!(""),
    };
    let pushes = axes.iter().map(|axis| {
        let field = &axis.field;
        quote! {
            let class = self.#field.class();
            if !class.is_empty() {
                if !list.is_empty() {
                    list.push(' ');
                }
                list.push_str(class);
            }
        }
    });
    let attributes = axes.iter().map(|axis| {
        let field = &axis.field;
        let attribute = format!("data-{}", kebab_case(&field.to_string()));
        quote!((#attribute, self.#field.name()))
    });
    let count = axes.len();

    quote! {
        #(#docs)*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
        #visibility struct #name {
            #(#fields)*
        }

        #(#declarations)*

        impl #name {
            /// The class every combination carries.
            pub const BASE: &'static str = #base_class;

            /// Every class this combination carries, in a stable order.
            ///
            /// The order is the base class followed by each axis in the order it was declared, so
            /// a class list is diffable and a transcript of one is deterministic.
            pub fn class_list(&self) -> ::std::string::String {
                let mut list = ::std::string::String::from(Self::BASE);
                #(#pushes)*
                list
            }

            /// The same classes, as a list an element takes.
            pub fn classes(&self) -> ::zgui::expansion::view::Classes {
                ::zgui::expansion::view::Classes::from(self.class_list())
            }

            /// This combination as `data-` attributes, one per axis.
            ///
            /// A stylesheet matches these rather than a concatenated class name, so a variant is
            /// selected for rather than built out of strings at run time.
            pub fn data_attributes(&self) -> [(&'static str, &'static str); #count] {
                [#(#attributes),*]
            }
        }
    }
}

/// The prefix each generated enumeration's name starts with.
///
/// `ButtonVariants` names `ButtonVariant` and `ButtonSize`, because the table is the set of a
/// component's axes and each axis is a type of its own.
fn prefix_of(name: &str) -> String {
    name.strip_suffix("Variants").unwrap_or(name).to_owned()
}

/// `variant` becomes `Variant`.
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

/// `SubOptimum` becomes `sub-optimum`.
fn kebab_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() {
            if index != 0 {
                out.push('-');
            }
            out.extend(character.to_lowercase());
        } else if character == '_' {
            out.push('-');
        } else {
            out.push(character);
        }
    }
    out
}
