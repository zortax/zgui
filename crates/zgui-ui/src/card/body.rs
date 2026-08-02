//! The body of a card, and the row of controls under it.

use zgui::prelude::*;
use zgui::{component, view};

use crate::card::SHEET;
use crate::card::style::CardStyle;

/// The body of a [`Card`](crate::Card).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Card {CardContent {text {"£42.00"}}} }
/// # }
/// ```
#[component]
pub fn CardContent(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CardStyle::CSS);
    view! {
        box(class = "zui-card__content", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// The row of controls at the bottom of a [`Card`](crate::Card).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Card {CardFooter {Button {"Pay"}}} }
/// # }
/// ```
#[component]
pub fn CardFooter(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CardStyle::CSS);
    view! {
        row(class = "zui-card__footer", {..attrs}, class = class) {{children.into_view_once()}}
    }
}
