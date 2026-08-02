//! The writing around a [`Field`](crate::Field)'s control.

use zgui::prelude::*;
use zgui::vocab::Live;
use zgui::{component, view};

use crate::field::TEXT_SHEET;
use crate::field::style::FieldTextStyle;

/// What a [`Field`](crate::Field) is asking for.
///
/// Relate it to the control with `control` so that pressing the words moves the keyboard to the
/// control and a reader announces the two together.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Field {FieldLabel {"Email"} Input()} }
/// # }
/// ```
#[component]
pub fn FieldLabel(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(TEXT_SHEET, FieldTextStyle::CSS);
    view! {
        label(class = "zui-field__label", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// The words beside a control inside a [`FieldContent`](crate::FieldContent).
///
/// The same weight as a [`FieldLabel`], and separate from it because these are not a label for
/// anything: the control they sit beside already carries its own name.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Field {FieldContent {FieldTitle {"Send me email"}}} }
/// # }
/// ```
#[component]
pub fn FieldTitle(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(TEXT_SHEET, FieldTextStyle::CSS);
    view! {
        box(class = "zui-field__title", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// What a person needs to know in order to answer well.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Field {FieldDescription {"We only use this to send you receipts."}} }
/// # }
/// ```
#[component]
pub fn FieldDescription(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(TEXT_SHEET, FieldTextStyle::CSS);
    view! {
        box(class = "zui-field__description", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// What is wrong with the answer.
///
/// It is announced the moment it appears, because somebody who cannot see the field turn red
/// would otherwise find out only by trying to send the form again.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Field(invalid = true) {FieldError {"That is not an address we recognise."}} }
/// # }
/// ```
#[component]
pub fn FieldError(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is wrong.
    children: Children,
) -> impl IntoView {
    install_stylesheet(TEXT_SHEET, FieldTextStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::Alert).live(Live::Polite));

    view! {
        box(class = "zui-field__error", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A line between two runs of fields, with a word on it when the break needs naming.
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
///         FieldSeparator {"Or pay another way"}
///         Field {FieldLabel {"Card number"} Input()}
///     }
/// }
/// # }
/// ```
///
/// # The words on it
///
/// They sit on the page's own colour and break the rule where they cross it, rather than the rule
/// being drawn in two pieces around them — so a separator with a word in it is one line of writing
/// whatever the word turns out to be, and nothing has to be measured to place it.
#[component]
pub fn FieldSeparator(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What names the break, when it needs naming.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    install_stylesheet(TEXT_SHEET, FieldTextStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::GenericContainer).hidden(true));
    let named = children.is_some();
    let said = children.map(|children| {
        view! {
            box(class = "zui-field__separator-content") {{children.into_view_once()}}
        }
    });

    view! {
        box(
            class = "zui-field__separator",
            attr:data-content = move || named.then(|| "true".to_owned()),
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-field__rule") {}
            {said.into_view()}
        }
    }
}
