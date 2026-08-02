//! The pieces an [`Empty`](crate::Empty) panel is built out of.

use zgui::prelude::*;
use zgui::{component, variants, view};

use crate::empty::SHEET;
use crate::empty::style::EmptyStyle;
use crate::support::variant_attrs;

variants! {
    /// The axes an [`EmptyMedia`] varies along.
    pub EmptyMediaVariants {
        base: "zui-empty__media",
        variant: { Default => "", Icon => "zui-empty__media--icon" } = Default,
    }
}

/// The mark, title and description at the top of an [`Empty`](crate::Empty) panel.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Empty {EmptyHeader {EmptyTitle {"No messages"}}} }
/// # }
/// ```
#[component]
pub fn EmptyHeader(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, EmptyStyle::CSS);
    view! {
        box(class = "zui-empty__header", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// The picture or mark above the title.
///
/// `Icon` puts it in a filled, rounded tile; the default leaves it as it is, which is what a
/// photograph or an illustration wants.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Empty {EmptyHeader {EmptyMedia(variant = EmptyMediaVariant::Icon) {"✉"}}} }
/// # }
/// ```
#[component]
pub fn EmptyMedia(
    /// How it is framed.
    #[prop(default = EmptyMediaVariant::Default)]
    variant: EmptyMediaVariant,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The mark.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, EmptyStyle::CSS);
    let variants = EmptyMediaVariants { variant };
    let own = variant_attrs(variants.classes(), variants.data_attributes())
        .a11y_from(A11yBinding::new(Role::GenericContainer).hidden(true));

    view! { box({..own}, {..attrs}, class = class) {{children.into_view_once()}} }
}

/// What is missing, in a few words.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Empty {EmptyHeader {EmptyTitle {"No messages"}}} }
/// # }
/// ```
#[component]
pub fn EmptyTitle(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, EmptyStyle::CSS);
    view! {
        label(class = "zui-empty__title", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// A line under the title explaining what would fill the region.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Empty {EmptyHeader {EmptyDescription {"Anything sent to you shows up here."}}} }
/// # }
/// ```
#[component]
pub fn EmptyDescription(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, EmptyStyle::CSS);
    view! {
        box(class = "zui-empty__description", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// What to do about the emptiness: the controls under the heading.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Empty {EmptyContent {Button {"Write one"}}} }
/// # }
/// ```
#[component]
pub fn EmptyContent(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The controls.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, EmptyStyle::CSS);
    view! {
        box(class = "zui-empty__content", {..attrs}, class = class) {{children.into_view_once()}}
    }
}
