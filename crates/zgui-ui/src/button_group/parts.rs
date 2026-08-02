//! The pieces a [`ButtonGroup`](crate::ButtonGroup) holds besides its buttons.

use zgui::prelude::*;
use zgui::{component, view};

use crate::button_group::SHEET;
use crate::button_group::style::ButtonGroupStyle;

/// A label sitting in the seam, drawn like a button but doing nothing.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     ButtonGroup {
///         ButtonGroupText {"https://"}
///         Input()
///     }
/// }
/// # }
/// ```
///
/// It is writing rather than a control, so it takes no focus and no reader announces it as
/// something to press — which is the whole reason it is not a disabled button.
#[component]
pub fn ButtonGroupText(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ButtonGroupStyle::CSS);
    view! {
        box(class = "zui-button-group__text", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A line dividing one part of a [`ButtonGroup`](crate::ButtonGroup) from the next.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     ButtonGroup {
///         Button(variant = ButtonVariant::Outline) {"Cut"}
///         ButtonGroupSeparator()
///         Button(variant = ButtonVariant::Outline) {"Paste"}
///     }
/// }
/// # }
/// ```
#[component]
pub fn ButtonGroupSeparator(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, ButtonGroupStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::GenericContainer).hidden(true));

    view! { box(class = "zui-button-group__separator", {..own}, {..attrs}, class = class) }
}
