//! The pieces a popover's panel is laid out with.

use zgui::prelude::*;
use zgui::{component, view};

use crate::popover::SHEET;
use crate::popover::style::PopoverStyle;

/// The heading of a popover: what it is about, and the line under it.
///
/// Smaller than a dialog's, and deliberately: a popover is read where it stands, beside the control
/// it belongs to, and a heading at a dialog's weight would out-shout the control that opened it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::popover::{PopoverDescription, PopoverDescriptionProps, PopoverHeader,
/// #     PopoverHeaderProps, PopoverTitle, PopoverTitleProps};
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Popover {PopoverContent {
///         PopoverHeader {
///             PopoverTitle {"Dimensions"}
///             PopoverDescription {"Applies to this frame only."}
///         }
///     }}
/// }
/// # }
/// ```
#[component]
pub fn PopoverHeader(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The title and the description.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, PopoverStyle::CSS);
    view! {
        column(class = "zui-popover__header", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// What a popover is about, in one line.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::popover::{PopoverTitle, PopoverTitleProps};
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Popover {PopoverContent {PopoverTitle {"Dimensions"}}} }
/// # }
/// ```
#[component]
pub fn PopoverTitle(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The title.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, PopoverStyle::CSS);
    view! {
        box(class = "zui-popover__title", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A line under a [`PopoverTitle`] qualifying it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::popover::{PopoverDescription, PopoverDescriptionProps};
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Popover {PopoverContent {PopoverDescription {"Applies here only."}}} }
/// # }
/// ```
#[component]
pub fn PopoverDescription(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The description.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, PopoverStyle::CSS);
    view! {
        box(class = "zui-popover__description", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
