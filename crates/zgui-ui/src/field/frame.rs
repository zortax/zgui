//! What holds a [`Field`](crate::Field), and what a field holds.

use zgui::prelude::*;
use zgui::{component, variants, view};

use crate::field::style::{FieldGroupStyle, FieldStyle};
use crate::field::{GROUP_SHEET, SHEET};
use crate::support::variant_attrs;

variants! {
    /// The axes a [`FieldLegend`] varies along.
    pub FieldLegendVariants {
        base: "zui-field__legend",
        variant: { Legend => "", Label => "zui-field__legend--label" } = Legend,
    }
}

/// Several [`Field`](crate::Field)s asking about one subject.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     FieldSet {
///         FieldLegend {"Delivery"}
///         FieldGroup {Field {FieldLabel {"Street"} Input()}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn FieldSet(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The legend, and the fields under it.
    children: Children,
) -> impl IntoView {
    install_stylesheet(GROUP_SHEET, FieldGroupStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::Group));

    view! {
        box(class = "zui-field__set", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// What a [`FieldSet`] is about.
///
/// `Legend` is the ordinary weight, for a heading over a section of a form. `Label` is the
/// smaller one, for a set that is really a single question with several answers — a row of
/// checkboxes, say — where a full heading would outweigh what it names.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { FieldSet {FieldLegend(variant = FieldLegendVariant::Label) {"Notify me about"}} }
/// # }
/// ```
#[component]
pub fn FieldLegend(
    /// How heavily it is set.
    #[prop(default = FieldLegendVariant::Legend)]
    variant: FieldLegendVariant,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(GROUP_SHEET, FieldGroupStyle::CSS);
    let variants = FieldLegendVariants { variant };
    let own = variant_attrs(variants.classes(), variants.data_attributes());

    view! { label({..own}, {..attrs}, class = class) {{children.into_view_once()}} }
}

/// A stack of [`Field`](crate::Field)s with the air between them that a form wants.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     FieldGroup {
///         Field {FieldLabel {"Street"} Input()}
///         Field {FieldLabel {"Town"} Input()}
///     }
/// }
/// # }
/// ```
#[component]
pub fn FieldGroup(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The fields.
    children: Children,
) -> impl IntoView {
    install_stylesheet(GROUP_SHEET, FieldGroupStyle::CSS);
    // A name of its own beside the sheet's, so that a group can be written about from *inside*
    // another group's rules — which is the only way a nested group can sit closer together than the
    // sections around it without the caller saying so.
    view! {
        box(
            class = FieldGroupStyle::CLASS,
            class = "zui-field__group",
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}

/// The words beside a control in a horizontal [`Field`](crate::Field).
///
/// A switch or a checkbox goes first and its writing goes here, which is what keeps the title and
/// the description stacked beside the control rather than wrapped under it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Field(orientation = FieldOrientation::Horizontal) {
///         Switch()
///         FieldContent {
///             FieldTitle {"Send me email"}
///             FieldDescription {"About once a month."}
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn FieldContent(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The title, and whatever qualifies it.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, FieldStyle::CSS);
    view! {
        box(class = "zui-field__content", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
