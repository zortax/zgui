//! The pieces an [`Item`](crate::Item) is built out of.

use zgui::prelude::*;
use zgui::{component, variants, view};

use crate::item::style::ItemPartStyle;
use crate::item::{ItemContext, PARTS_SHEET};
use crate::support::variant_attrs;

variants! {
    /// The axes an [`ItemMedia`] varies along.
    pub ItemMediaVariants {
        base: "zui-item__media",
        variant: {
            Default => "",
            Icon => "zui-item__media--icon",
            Image => "zui-item__media--image",
        } = Default,
    }
}

/// The picture or mark at the start of an [`Item`](crate::Item).
///
/// `Icon` puts it in a small bordered tile, `Image` crops it into a rounded square, and the
/// default leaves it exactly as it is.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Item {ItemMedia(variant = ItemMediaVariant::Icon) {"📄"}} }
/// # }
/// ```
#[component]
pub fn ItemMedia(
    /// How it is framed.
    #[prop(default = ItemMediaVariant::Default)]
    variant: ItemMediaVariant,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The mark.
    children: Children,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, ItemPartStyle::CSS);
    let variants = ItemMediaVariants { variant };
    let own = variant_attrs(variants.classes(), variants.data_attributes());

    view! { box({..own}, {..attrs}, class = class) {{children.into_view_once()}} }
}

/// The words in the middle of an [`Item`](crate::Item), stacked.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Item {ItemContent {ItemTitle {"report.pdf"}}} }
/// # }
/// ```
#[component]
pub fn ItemContent(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The title, and whatever qualifies it.
    children: Children,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, ItemPartStyle::CSS);
    view! {
        box(class = "zui-item__content", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// What an [`Item`](crate::Item) is, in a few words.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Item {ItemContent {ItemTitle {"report.pdf"}}} }
/// # }
/// ```
#[component]
pub fn ItemTitle(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, ItemPartStyle::CSS);
    view! {
        label(class = "zui-item__title", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// A line under an [`ItemTitle`] qualifying it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Item {ItemContent {ItemDescription {"1.2 MB"}}} }
/// # }
/// ```
#[component]
pub fn ItemDescription(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, ItemPartStyle::CSS);
    if let Some(item) = ItemContext::current() {
        item.claim_description();
    }
    view! {
        box(class = "zui-item__description", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// The controls at the end of an [`Item`](crate::Item).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Item {ItemActions {Button {"Remove"}}} }
/// # }
/// ```
#[component]
pub fn ItemActions(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The controls.
    children: Children,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, ItemPartStyle::CSS);
    view! {
        box(class = "zui-item__actions", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// A full-width row above the rest of an [`Item`](crate::Item).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Item {ItemHeader {Badge {"New"}}} }
/// # }
/// ```
#[component]
pub fn ItemHeader(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, ItemPartStyle::CSS);
    view! {
        box(class = "zui-item__header", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// A full-width row below the rest of an [`Item`](crate::Item).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Item {ItemFooter {Badge {"Draft"}}} }
/// # }
/// ```
#[component]
pub fn ItemFooter(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, ItemPartStyle::CSS);
    view! {
        box(class = "zui-item__footer", {..attrs}, class = class) {{children.into_view_once()}}
    }
}
