//! A small piece of text that labels something else.

mod style;

pub use crate::badge::style::BadgeStyle;

use zgui::prelude::*;
use zgui::{component, variants, view};

use crate::support::variant_attrs;

/// What the badge's rules are installed under.
const SHEET: &str = "zui-badge";

variants! {
    /// The axes a [`Badge`] varies along.
    pub BadgeVariants {
        base: "zui-badge",
        variant: {
            Default => "zui-badge--default",
            Secondary => "zui-badge--secondary",
            Destructive => "zui-badge--destructive",
            Outline => "zui-badge--outline",
            Ghost => "zui-badge--ghost",
            Link => "zui-badge--link",
        } = Default,
    }
}

/// A count, a status or a category, in a pill beside whatever it belongs to.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A row naming a build and how it went.
/// #[component]
/// fn Build() -> impl IntoView {
///     view! {
///         row {
///             text {"main"}
///             Badge(variant = BadgeVariant::Destructive) {"failed"}
///         }
///     }
/// }
/// ```
///
/// # What a reader is told
///
/// By default, only the text — a badge is a piece of writing, not a control, and a role would make
/// a reader announce it as something to operate. A badge whose meaning is *not* in its text, or
/// which stands for something elsewhere on the surface, says so with `a11y:label` or
/// `a11y:role` like any other element.
#[component]
pub fn Badge(
    /// How it looks.
    #[prop(default = BadgeVariant::Default)]
    variant: BadgeVariant,
    /// Classes merged after the badge's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, BadgeStyle::CSS);
    let variants = BadgeVariants { variant };
    let own = variant_attrs(variants.classes(), variants.data_attributes());

    view! {
        box(class = BadgeStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
