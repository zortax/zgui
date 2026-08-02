//! One prop: what it is called, what it takes, and whether it may be left out.

use proc_macro2::Span;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Attribute, Expr, Ident, Token, Type, Visibility};

/// Whether a prop must be given, and what stands in for it when it is not.
pub(crate) enum Requirement {
    /// The prop must be given, and leaving it out is a compile error naming it.
    Required,
    /// The prop may be left out, and is its type's default when it is.
    Optional,
    /// The prop may be left out, and is this expression when it is.
    Default(Box<Expr>),
    /// The prop receives whatever the caller forwarded, and is empty when nothing was.
    Attrs,
}

/// How a value reaches the prop's type.
enum Conversion {
    /// The setter takes the type itself.
    Exact,
    /// The setter takes anything that converts with `Into`.
    Into,
    /// The setter takes a literal, a signal or a closure.
    Reactive(Box<Type>),
    /// The setter takes a closure returning a view, called once.
    ChildrenOnce,
    /// The setter takes a closure returning a view, called whenever the view is rebuilt.
    ChildrenMany,
}

/// One prop of a component or a slot.
pub(crate) struct Prop {
    /// The documentation written on it, carried to both the field and the setter.
    pub(crate) docs: Vec<Attribute>,
    /// The field it becomes.
    pub(crate) field: Ident,
    /// What a caller writes to set it.
    pub(crate) setter: Ident,
    /// Its type.
    pub(crate) ty: Type,
    /// Whether it must be given.
    pub(crate) requirement: Requirement,
    /// How a value reaches its type.
    conversion: Conversion,
    /// Where it was written.
    pub(crate) span: Span,
}

impl Prop {
    /// Reads one prop from a name, a type and the attributes written on it.
    pub(crate) fn new(field: Ident, ty: Type, attributes: &[Attribute]) -> syn::Result<Self> {
        let span = field.span();
        let mut docs = Vec::new();
        let mut requirement = None;
        let mut into = false;
        let mut renamed = None;
        for attribute in attributes {
            if attribute.path().is_ident("doc") {
                docs.push(attribute.clone());
                continue;
            }
            if !attribute.path().is_ident("prop") {
                return Err(syn::Error::new(
                    attribute.span(),
                    "a prop takes `#[prop(…)]` and documentation, and nothing else",
                ));
            }
            attribute.parse_nested_meta(|meta| {
                let path = meta
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                match path.as_str() {
                    "into" => {
                        into = true;
                        Ok(())
                    }
                    "optional" => {
                        requirement = Some(Requirement::Optional);
                        Ok(())
                    }
                    "attrs" => {
                        requirement = Some(Requirement::Attrs);
                        Ok(())
                    }
                    "default" => {
                        let value = meta.value()?;
                        requirement = Some(Requirement::Default(Box::new(value.parse()?)));
                        Ok(())
                    }
                    "name" => {
                        let value = meta.value()?;
                        let name = value.parse::<syn::LitStr>()?;
                        renamed = Some(name);
                        Ok(())
                    }
                    other => Err(meta.error(format!(
                        "`{other}` is not a prop attribute\n\n\
                         note: the prop attributes are `into`, `optional`, `default = …`, \
                         `name = \"…\"` and `attrs`"
                    ))),
                }
            })?;
        }
        let setter = match &renamed {
            Some(name) => syn::parse_str::<Ident>(&name.value())
                .or_else(|_| syn::parse_str::<Ident>(&format!("r#{}", name.value())))
                .map_err(|_| syn::Error::new(name.span(), "a prop's name is an identifier"))?,
            None => field.clone(),
        };
        let conversion = conversion(&ty, into);
        let requirement = match requirement {
            Some(requirement) => requirement,
            None => Requirement::Required,
        };
        // A forwarded bundle is always written `{..attrs}` at the call site, whatever the prop
        // holding it is called, so its setter has the one name the expansion can rely on.
        let setter = match (&requirement, &renamed) {
            (Requirement::Attrs, None) => Ident::new("attrs", span),
            _ => setter,
        };
        Ok(Self {
            docs,
            field,
            setter,
            ty,
            requirement,
            conversion,
            span,
        })
    }

    /// Whether leaving this prop out is an error.
    pub(crate) fn is_required(&self) -> bool {
        matches!(self.requirement, Requirement::Required)
    }

    /// The parameter the setter takes, and the expression that stores it.
    ///
    /// The second element is written in terms of `value`, which is the parameter's name.
    pub(crate) fn setter_signature(&self) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
        let ty = &self.ty;
        let stored = |value: proc_macro2::TokenStream| match self.optional_inner() {
            Some(_) => quote!(::core::option::Option::Some(#value)),
            None => value,
        };
        match &self.conversion {
            // An optional prop with no conversion takes *either* spelling: `today=day` is what an
            // author writes, and `today=maybe_day` is what a component wrapping another one has —
            // a wrapper's own optional prop is an `Option<T>`, and a setter that only took `T`
            // would leave it with no way at all to pass its own prop on.
            Conversion::Exact => match self.optional_inner() {
                Some(inner) => (
                    quote!(value: impl ::core::convert::Into<::core::option::Option<#inner>>),
                    quote!(::core::convert::Into::into(value)),
                ),
                None => (quote!(value: #ty), quote!(value)),
            },
            Conversion::Into => {
                let ty = self.optional_inner().unwrap_or_else(|| ty.clone());
                (
                    quote!(value: impl ::core::convert::Into<#ty>),
                    stored(quote!(::core::convert::Into::into(value))),
                )
            }
            Conversion::Reactive(inner) => (
                quote!(value: impl ::zgui::expansion::view::IntoReactiveValue<#inner, __ValueShape>),
                stored(quote!(
                    ::zgui::expansion::view::IntoReactiveValue::into_reactive_value(value)
                )),
            ),
            Conversion::ChildrenOnce => (
                quote!(value: impl ::core::ops::FnOnce() -> ::zgui::expansion::view::AnyView + 'static),
                stored(quote!(::zgui::expansion::view::Children::new(value))),
            ),
            Conversion::ChildrenMany => (
                quote!(value: impl ::core::ops::Fn() -> ::zgui::expansion::view::AnyView + 'static),
                stored(quote!(::zgui::expansion::view::ChildrenFn::new(value))),
            ),
        }
    }

    /// The extra type parameter the setter needs, when its conversion has one.
    pub(crate) fn setter_generics(&self) -> Option<proc_macro2::TokenStream> {
        matches!(self.conversion, Conversion::Reactive(_)).then(|| quote!(__ValueShape))
    }

    /// What the prop is when it was not given.
    pub(crate) fn fallback(&self) -> proc_macro2::TokenStream {
        match &self.requirement {
            Requirement::Required => {
                let message = format!("the `{}` prop is required", self.setter);
                quote!(::core::option::Option::expect(value, #message))
            }
            Requirement::Optional => {
                quote!(::core::option::Option::unwrap_or_default(value))
            }
            Requirement::Default(default) => {
                quote!(::core::option::Option::unwrap_or_else(value, || #default))
            }
            Requirement::Attrs => {
                quote!(::core::option::Option::unwrap_or_default(value))
            }
        }
    }

    /// The inner type of an `Option<T>` prop that may be left out.
    ///
    /// Such a prop is set with a `T`, because `icon=path` is what an author writes and
    /// `icon=Some(path)` is noise the macro can remove.
    fn optional_inner(&self) -> Option<Type> {
        if matches!(self.requirement, Requirement::Required | Requirement::Attrs) {
            return None;
        }
        option_inner(&self.ty)
    }
}

/// How values reach `ty`.
///
/// Looked for through an `Option` as well as at the type itself, because `Option<Children>` is how
/// a prop says *these children may be left out* — a separator with a default glyph, a message with
/// a fallback. Deciding from the outer type alone would give that prop a setter taking a built
/// value, and a caller writing children into it would be told its closure is not a `Children`.
fn conversion(ty: &Type, into: bool) -> Conversion {
    let inner = option_inner(ty);
    let names = [last_segment(ty), inner.as_ref().and_then(last_segment)];
    for name in names.iter().flatten() {
        match name.as_str() {
            "Children" => return Conversion::ChildrenOnce,
            "ChildrenFn" => return Conversion::ChildrenMany,
            _ => {}
        }
    }
    if !into {
        return Conversion::Exact;
    }
    match generic_argument(ty, "ReactiveValue") {
        Some(inner) => Conversion::Reactive(Box::new(inner)),
        None => Conversion::Into,
    }
}

/// The inner type of `Option<T>`.
fn option_inner(ty: &Type) -> Option<Type> {
    generic_argument(ty, "Option")
}

/// The single generic argument of `Name<T>`, when `ty` is one.
fn generic_argument(ty: &Type, name: &str) -> Option<Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != name {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    arguments.args.iter().find_map(|argument| match argument {
        syn::GenericArgument::Type(inner) => Some(inner.clone()),
        _ => None,
    })
}

/// The name of a type's last path segment.
fn last_segment(ty: &Type) -> Option<String> {
    let Type::Path(path) = ty else {
        return None;
    };
    Some(path.path.segments.last()?.ident.to_string())
}

/// Reads the props of a `#[slot]` struct's fields.
pub(crate) fn from_fields(fields: &syn::Fields) -> syn::Result<(Vec<Prop>, Vec<Visibility>)> {
    let syn::Fields::Named(named) = fields else {
        return Err(syn::Error::new(
            fields.span(),
            "a slot's props are named fields",
        ));
    };
    let mut props = Vec::new();
    let mut visibilities = Vec::new();
    for field in &named.named {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new(field.span(), "a slot's props are named fields"))?;
        props.push(Prop::new(ident, field.ty.clone(), &field.attrs)?);
        visibilities.push(field.vis.clone());
    }
    Ok((props, visibilities))
}

/// Reads the props of a component's arguments.
pub(crate) fn from_arguments(
    inputs: &syn::punctuated::Punctuated<syn::FnArg, Token![,]>,
) -> syn::Result<Vec<Prop>> {
    let mut props = Vec::new();
    for input in inputs {
        let syn::FnArg::Typed(typed) = input else {
            return Err(syn::Error::new(
                input.span(),
                "a component is a free function and takes no `self`",
            ));
        };
        let syn::Pat::Ident(pattern) = &*typed.pat else {
            return Err(syn::Error::new(
                typed.pat.span(),
                "a prop is named, so it is written as one identifier",
            ));
        };
        if matches!(&*typed.ty, Type::ImplTrait(_)) {
            return Err(syn::Error::new(
                typed.ty.span(),
                "a prop's type is stored in the props struct, and `impl Trait` cannot be\n\n\
                 help: name a type parameter instead: \
                 `fn Thing<F: Fn() -> bool>(each: F)`",
            ));
        }
        props.push(Prop::new(
            pattern.ident.clone(),
            (*typed.ty).clone(),
            &typed.attrs,
        )?);
    }
    Ok(props)
}
