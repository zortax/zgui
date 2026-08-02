//! The pieces a dialog's surface is laid out with.

use zgui::prelude::*;
use zgui::{component, view};

use crate::button::ButtonVariant;
use crate::dialog::SHEET;
use crate::dialog::style::DialogStyle;
use crate::dialog::trigger::DialogCloseProps;
use crate::overlay::SurfaceLabels;

/// The heading of a dialog: its title, and the line under it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Dialog {DialogContent {
///         DialogHeader {DialogTitle {"Delete"}}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn DialogHeader(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The title and description.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, DialogStyle::CSS);
    view! {
        column(class = "zui-dialog__header", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// What a dialog is about, in one line — and what names it to a reader.
///
/// Writing one is what makes the surface labelled: the element binds itself to the handle the
/// dialog published on the way down, and the surface points at it. A dialog with no title is
/// announced as an unlabelled dialog rather than as the last thing that happened to have a name.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Dialog {DialogContent {DialogTitle {"Delete project"}}} }
/// # }
/// ```
#[component]
pub fn DialogTitle(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The title.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, DialogStyle::CSS);
    let node = SurfaceLabels::current()
        .map(|labels| labels.title())
        .unwrap_or_default();
    view! {
        label(node_ref = node, class = "zui-dialog__title", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A line under a [`DialogTitle`](crate::DialogTitle) qualifying it, which also describes the
/// surface to a reader.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Dialog {DialogContent {
///         DialogDescription {"This cannot be undone."}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn DialogDescription(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The description.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, DialogStyle::CSS);
    let node = SurfaceLabels::current()
        .map(|labels| labels.description())
        .unwrap_or_default();
    view! {
        box(node_ref = node, class = "zui-dialog__description", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// The row of controls at the bottom of a dialog.
///
/// A dialog whose only answer is "I have read this" does not need a control written for it:
/// `close_control` draws the one it would have been.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Dialog {DialogContent {
///         DialogFooter {DialogClose {"Cancel"}Button {"Save"}}
///         DialogFooter(close_control = true) {Button {"Save"}}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn DialogFooter(
    /// Whether the row ends with a "Close" of its own.
    #[prop(default = false)]
    close_control: bool,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The controls.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, DialogStyle::CSS);
    view! {
        row(class = "zui-dialog__footer", {..attrs}, class = class) {
            {children.into_view_once()}
            if move || close_control {
                DialogClose(variant = {ButtonVariant::Outline}) {"Close"}
            } else {}
        }
    }
}
