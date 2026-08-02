//! Everything that can be written in a node's attribute list.

pub(crate) mod event;
pub(crate) mod name;
pub(crate) mod state;

use proc_macro2::Span;
use syn::parse::ParseStream;
use syn::{Token, braced, token};

use crate::view::attr::event::Modifiers;
use crate::view::attr::name::Name;
use crate::view::value::Value;

/// One attribute, prop, listener or spread.
pub(crate) enum Attr {
    /// `name=value`, or `name` for the shorthand: a typed attribute or a component prop.
    Named { name: Name, value: Value },
    /// `class=value`: the whole class list.
    Class(Value),
    /// `class:name=value`: one class, toggled.
    ClassToggle { name: Name, value: Value },
    /// `style=value`: the whole inline style text.
    StyleText(Value),
    /// `style:property=value`: one declaration.
    StyleProperty { name: Name, value: Value },
    /// `var:--name=value`: one custom property.
    CustomProperty { name: Name, value: Value },
    /// `attr:name=value`: an arbitrary attribute, visible to selector matching.
    Attribute { name: Name, value: Value },
    /// `prop:name=value`: an imperative element property.
    Property { name: Name, value: Value },
    /// `state:name=value`: one of the states a view may assert.
    State { name: Name, value: Value },
    /// `custom_state:name=value`: an author-defined state.
    CustomState { name: Name, value: Value },
    /// `on:event=closure`: a listener.
    Listener {
        /// The event listened for.
        name: Name,
        /// What `:capture`, `:once` and friends asked for.
        modifiers: Modifiers,
        /// The handler.
        value: Value,
    },
    /// `a11y:name=value`: one accessibility property.
    A11y { name: Name, value: Value },
    /// `node_ref=value`: where to record this element's node once it exists.
    NodeRef(Value),
    /// `let:name`: names the argument a component passes to its children.
    Let(syn::Ident),
    /// `slot` or `slot="name"`: this component fills the named slot of its parent.
    Slot { name: Option<String>, span: Span },
    /// `{..expr}`: a forwarded bundle, replayed here.
    Spread(Value),
}

/// The namespaces a name may carry.
const NAMESPACES: &[&str] = &[
    "class",
    "style",
    "var",
    "attr",
    "prop",
    "state",
    "custom_state",
    "on",
    "a11y",
    "let",
];

impl Attr {
    /// Where this attribute was written.
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::Named { name, .. }
            | Self::ClassToggle { name, .. }
            | Self::StyleProperty { name, .. }
            | Self::CustomProperty { name, .. }
            | Self::Attribute { name, .. }
            | Self::Property { name, .. }
            | Self::State { name, .. }
            | Self::CustomState { name, .. }
            | Self::Listener { name, .. }
            | Self::A11y { name, .. } => name.span,
            Self::Class(value)
            | Self::StyleText(value)
            | Self::NodeRef(value)
            | Self::Spread(value) => value.span,
            Self::Let(ident) => ident.span(),
            Self::Slot { span, .. } => *span,
        }
    }

    /// Parses one attribute.
    pub(crate) fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(token::Brace) {
            return Self::parse_spread(input);
        }
        let first = Name::parse(input)?;
        if input.peek(Token![:]) && !input.peek(Token![::]) {
            input.parse::<Token![:]>()?;
            return Self::parse_namespaced(input, first);
        }
        match first.text.as_str() {
            "class" => Ok(Self::Class(Self::value(input, &first)?)),
            "style" => Ok(Self::StyleText(Self::value(input, &first)?)),
            "node_ref" => Ok(Self::NodeRef(Self::value(input, &first)?)),
            "slot" => {
                let span = first.span;
                if !input.peek(Token![=]) {
                    return Ok(Self::Slot { name: None, span });
                }
                input.parse::<Token![=]>()?;
                let value = Value::parse(input)?;
                match &value.expr {
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(name),
                        ..
                    }) => Ok(Self::Slot {
                        name: Some(name.value()),
                        span,
                    }),
                    _ => Err(syn::Error::new(
                        value.span,
                        "a slot is named with a string literal: `slot=\"header\"`",
                    )),
                }
            }
            _ => {
                let value = Self::value(input, &first)?;
                Ok(Self::Named { name: first, value })
            }
        }
    }

    /// Parses what follows a `namespace:`.
    fn parse_namespaced(input: ParseStream<'_>, namespace: Name) -> syn::Result<Self> {
        if namespace.text == "let" {
            let ident = input.parse::<syn::Ident>()?;
            return Ok(Self::Let(ident));
        }
        if namespace.text == "on" {
            let name = Name::parse(input)?;
            let modifiers = Modifiers::parse(input)?;
            input.parse::<Token![=]>().map_err(|_| {
                syn::Error::new(
                    name.span,
                    format!("`on:{}` needs a handler: `on:{}=…`", name.text, name.text),
                )
            })?;
            let value = Value::parse(input)?;
            return Ok(Self::Listener {
                name,
                modifiers,
                value,
            });
        }
        let name = Name::parse(input)?;
        let value = match namespace.text.as_str() {
            "class" | "state" | "custom_state" => Self::toggle_value(input, &name)?,
            _ => Self::value(input, &name)?,
        };
        match namespace.text.as_str() {
            "class" => Ok(Self::ClassToggle { name, value }),
            "style" => Ok(Self::StyleProperty { name, value }),
            "var" => Ok(Self::CustomProperty { name, value }),
            "attr" => Ok(Self::Attribute { name, value }),
            "prop" => Ok(Self::Property { name, value }),
            "state" => Ok(Self::State { name, value }),
            "custom_state" => Ok(Self::CustomState { name, value }),
            "a11y" => Ok(Self::A11y { name, value }),
            other => Err(syn::Error::new(
                namespace.span,
                format!(
                    "`{other}:` is not an attribute namespace\n\n\
                     note: the namespaces are {}",
                    NAMESPACES
                        .iter()
                        .map(|namespace| format!("`{namespace}:`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
        }
    }

    /// Parses `{..expr}`.
    ///
    /// The help names where a braced child belongs instead, because the two are one delimiter
    /// apart and the mistake is otherwise reported against the bundle's type.
    fn parse_spread(input: ParseStream<'_>) -> syn::Result<Self> {
        let content;
        let brace = braced!(content in input);
        if !content.peek(Token![..]) {
            return Err(syn::Error::new(
                brace.span.join(),
                "a braced attribute spreads a forwarded bundle and must start with `..`: \
                 `{..attrs}`\n\n\
                 help: a braced *child* goes in the block, not in the parentheses",
            ));
        }
        content.parse::<Token![..]>()?;
        if content.is_empty() {
            return Err(syn::Error::new(
                brace.span.join(),
                "`{..}` spreads nothing\n\n\
                 help: name the bundle to spread: `{..attrs}`",
            ));
        }
        let expr = content.parse::<syn::Expr>()?;
        if !content.is_empty() {
            return Err(syn::Error::new(
                content.span(),
                "a spread carries one bundle: `{..attrs}`",
            ));
        }
        Ok(Self::Spread(Value {
            span: brace.span.join(),
            expr,
        }))
    }

    /// Parses `=value`, or the `name` shorthand when there is no `=`.
    fn value(input: ParseStream<'_>, name: &Name) -> syn::Result<Value> {
        if input.parse::<Option<Token![=]>>()?.is_some() {
            return Value::parse(input);
        }
        Value::shorthand(&name.text, name.span)
    }

    /// Parses `=value`, or `true` when a toggle is written on its own.
    fn toggle_value(input: ParseStream<'_>, name: &Name) -> syn::Result<Value> {
        if input.parse::<Option<Token![=]>>()?.is_some() {
            return Value::parse(input);
        }
        Ok(Value {
            expr: syn::parse_quote!(true),
            span: name.span,
        })
    }
}
