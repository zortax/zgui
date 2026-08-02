//! The label, the hint and the message around one field.

use zgui::prelude::*;
use zgui::{component, view};

use crate::form::SHEET;
use crate::form::field::FormFieldContext;
use crate::form::style::FormStyle;
use crate::label::LabelProps;

/// One field's parts, stacked.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Form {FormField(name = "name") {
///         FormItem {FormLabel {"Name"}FormMessage()}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn FormItem(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The label, the control, the hint and the message.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, FormStyle::CSS);
    view! {
        column(class = "zui-form__item", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// The name of one field's control.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Form {FormField(name = "name") {
///         FormItem {FormLabel {"Name"}}
///     }}
/// }
/// # }
/// ```
///
/// It names the field's control without being told which element that is: the field published it,
/// so a caller cannot wire the two together wrongly and cannot forget to wire them at all.
#[component]
pub fn FormLabel(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the field is called.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, FormStyle::CSS);
    let field = FormFieldContext::current();
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-form__label"), true)
        .attribute(zgui::view::AttrName::new("data-invalid"), move || {
            field
                .and_then(FormFieldContext::shown_error)
                .is_some()
                .then(|| "true".to_owned())
        });

    view! {
        Label(
            node_ref = field.map(FormFieldContext::label).unwrap_or_default(),
            control = field.map(FormFieldContext::control).unwrap_or_default(),
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}

/// The line under one field's control saying what belongs in it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Form {FormField(name = "name") {
///         FormItem {FormDescription {"As it appears on the card."}}
///     }}
/// }
/// # }
/// ```
///
/// The control points its description at this line while the value is fine, and at the message
/// once it is not.
#[component]
pub fn FormDescription(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The hint.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, FormStyle::CSS);
    let field = FormFieldContext::current();

    view! {
        box(
            class = "zui-form__description",
            node_ref = field.map(FormFieldContext::description).unwrap_or_default(),
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}

/// The line saying what is wrong with one field.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Form {FormField(name = "name") {
///         FormItem {FormLabel {"Name"}FormMessage()}
///     }}
/// }
/// # }
/// ```
///
/// It shows whatever the field's rule said, or the children when there is no rule to say anything.
/// The element is always there — it is what the control's `described_by` names once the value goes
/// wrong, and a relation to something that comes and goes is a relation that is sometimes wrong —
/// but it says nothing until there is something to say.
#[component]
pub fn FormMessage(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What to say when the field has no rule of its own.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    install_stylesheet(SHEET, FormStyle::CSS);
    let field = FormFieldContext::current();
    let shown = move || field.and_then(FormFieldContext::shown_error);

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if shown().is_some() { "error" } else { "quiet" }.to_owned())
        })
        .a11y_from(
            A11yBinding::new(Role::Alert)
                .live(zgui::vocab::Live::Polite)
                .hidden(move || shown().is_none()),
        );

    let fallback = children.map(Children::into_view_once);

    view! {
        box(
            class = "zui-form__message",
            node_ref = field.map(FormFieldContext::message).unwrap_or_default(),
            {..own},
            {..attrs},
            class = class
        ) {
            {move || shown().unwrap_or_default()}
            {fallback}
        }
    }
}
