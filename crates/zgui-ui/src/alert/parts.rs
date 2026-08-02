//! The two pieces of writing inside an [`Alert`](crate::Alert).

use zgui::prelude::*;
use zgui::{component, view};

use crate::alert::SHEET;
use crate::alert::style::AlertStyle;

/// The heading of an [`Alert`](crate::Alert).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Notice() -> impl IntoView {
/// view! { Alert {AlertTitle {"Saved"}} }
/// # }
/// ```
#[component]
pub fn AlertTitle(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AlertStyle::CSS);
    view! {
        label(class = "zui-alert__title", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// The body of an [`Alert`](crate::Alert), under its title.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Notice() -> impl IntoView {
/// view! { Alert {AlertDescription {"Everything is up to date."}} }
/// # }
/// ```
#[component]
pub fn AlertDescription(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AlertStyle::CSS);
    view! {
        box(class = "zui-alert__description", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
