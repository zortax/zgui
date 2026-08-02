//! The bands a sheet's panel is laid out in.

use zgui::prelude::*;
use zgui::{component, view};

use crate::overlay::SurfaceLabels;
use crate::sheet::SHEET_NAME;
use crate::sheet::style::SheetStyle;

/// The band at the top of a sheet: its title, and the line under it.
///
/// It pads itself rather than being padded by the panel, because a sheet's panel is a stack of
/// full-width bands and a panel that inset them could not hold one that reaches the edges.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Sheet {SheetContent {
///         SheetHeader {
///             SheetTitle {"Invoice 4471"}
///             SheetDescription {"Issued 3 March, due 17 March."}
///         }
///     }}
/// }
/// # }
/// ```
#[component]
pub fn SheetHeader(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The title and the description.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET_NAME, SheetStyle::CSS);
    view! {
        column(class = "zui-sheet__header", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// The band at the bottom of a sheet, holding whatever answers it.
///
/// Pushed to the bottom of the panel rather than laid out under the content, so a sheet with one
/// short paragraph in it still has its controls where the eye goes looking for them.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Sheet {SheetContent {
///         SheetFooter {SheetClose {"Close"}}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn SheetFooter(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The controls.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET_NAME, SheetStyle::CSS);
    view! {
        column(class = "zui-sheet__footer", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// What a sheet is for, in one line — and what names it to a reader.
///
/// Writing one is what makes the panel labelled: the element binds itself to the handle the sheet
/// published on the way down, and the panel points at it. A sheet with no title is announced as an
/// unlabelled dialog rather than as the last thing that happened to have a name.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Sheet {SheetContent {SheetTitle {"Invoice 4471"}}} }
/// # }
/// ```
#[component]
pub fn SheetTitle(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The title.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET_NAME, SheetStyle::CSS);
    let node = SurfaceLabels::current()
        .map(|labels| labels.title())
        .unwrap_or_default();
    view! {
        label(node_ref = node, class = "zui-sheet__title", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A line under a [`SheetTitle`] qualifying it, which also describes the panel to a reader.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Sheet {SheetContent {
///         SheetDescription {"Issued 3 March, due 17 March."}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn SheetDescription(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The description.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET_NAME, SheetStyle::CSS);
    let node = SurfaceLabels::current()
        .map(|labels| labels.description())
        .unwrap_or_default();
    view! {
        box(node_ref = node, class = "zui-sheet__description", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
