//! The table a `variants!` invocation is written as.

use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Ident, LitStr, Token, Visibility, braced};

/// One choice on one axis: what it is called, and the class it adds.
pub(crate) struct Choice {
    /// The variant of the generated enumeration.
    pub(crate) name: Ident,
    /// The class this choice adds, which may be empty.
    pub(crate) class: LitStr,
}

/// One axis of variation: a set of choices and the one that is the default.
pub(crate) struct Axis {
    /// The field, in snake case.
    pub(crate) field: Ident,
    /// The choices, in the order they were written.
    pub(crate) choices: Vec<Choice>,
    /// Which choice is the default.
    pub(crate) default: Ident,
}

/// A whole `variants!` table.
pub(crate) struct Table {
    /// What the table is documented as.
    pub(crate) docs: Vec<Attribute>,
    /// How visible the generated types are.
    pub(crate) visibility: Visibility,
    /// The name of the generated struct.
    pub(crate) name: Ident,
    /// The class every combination carries.
    pub(crate) base: Option<LitStr>,
    /// The axes, in the order they were written.
    pub(crate) axes: Vec<Axis>,
}

impl Parse for Table {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let docs = Attribute::parse_outer(input)?;
        let visibility = input.parse::<Visibility>()?;
        let name = input.parse::<Ident>()?;
        let body;
        braced!(body in input);

        let mut base = None;
        let mut axes: Vec<Axis> = Vec::new();
        while !body.is_empty() {
            let field = body.parse::<Ident>()?;
            body.parse::<Token![:]>()?;
            if field == "base" {
                if base.is_some() {
                    return Err(syn::Error::new(field.span(), "`base` is written once"));
                }
                base = Some(body.parse::<LitStr>()?);
            } else {
                axes.push(parse_axis(&body, field)?);
            }
            if body.is_empty() {
                break;
            }
            body.parse::<Token![,]>()?;
        }
        if axes.is_empty() {
            return Err(syn::Error::new(
                name.span(),
                "a variants table has at least one axis: `variant: { … } = Default`",
            ));
        }
        Ok(Self {
            docs,
            visibility,
            name,
            base,
            axes,
        })
    }
}

/// Parses `axis: { Name => "class", … } = Default`.
fn parse_axis(input: ParseStream<'_>, field: Ident) -> syn::Result<Axis> {
    let choices_body;
    braced!(choices_body in input);
    let mut choices = Vec::new();
    while !choices_body.is_empty() {
        let name = choices_body.parse::<Ident>()?;
        choices_body.parse::<Token![=>]>()?;
        let class = choices_body.parse::<LitStr>()?;
        choices.push(Choice { name, class });
        if choices_body.is_empty() {
            break;
        }
        choices_body.parse::<Token![,]>()?;
    }
    if choices.is_empty() {
        return Err(syn::Error::new(
            field.span(),
            format!("`{field}` has no choices"),
        ));
    }
    input.parse::<Token![=]>().map_err(|_| {
        syn::Error::new(
            field.span(),
            format!(
                "`{field}` has no default\n\n\
                 help: name the choice that applies when nothing is chosen: `= {}`",
                choices[0].name
            ),
        )
    })?;
    let default = input.parse::<Ident>()?;
    if !choices.iter().any(|choice| choice.name == default) {
        return Err(syn::Error::new(
            default.span(),
            format!("`{default}` is not one of `{field}`'s choices"),
        ));
    }
    Ok(Axis {
        field,
        choices,
        default,
    })
}
