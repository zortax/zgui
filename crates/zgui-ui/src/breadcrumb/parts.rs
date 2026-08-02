//! The pieces a breadcrumb trail is written out of.

use zgui::prelude::*;
use zgui::vocab::AriaCurrent;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::{chevron::CHEVRON_RIGHT, ui::ELLIPSIS};

use crate::breadcrumb::SHEET;
use crate::breadcrumb::style::BreadcrumbStyle;

/// The list a [`Breadcrumb`](crate::Breadcrumb) is made of.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Breadcrumb {
///         BreadcrumbList {
///             BreadcrumbItem {BreadcrumbPage {"Billing"}}
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn BreadcrumbList(
    /// Classes merged after the list's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The entries and the separators between them.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, BreadcrumbStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::List));

    view! {
        row(class = "zui-breadcrumb__list", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// One entry of a [`BreadcrumbList`].
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Breadcrumb {BreadcrumbList {
///         BreadcrumbItem {BreadcrumbPage {"Billing"}}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn BreadcrumbItem(
    /// Classes merged after the entry's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The link, or the page.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, BreadcrumbStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::ListItem));

    view! {
        row(class = "zui-breadcrumb__item", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// An entry of the trail that goes somewhere.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Breadcrumb {BreadcrumbList {BreadcrumbItem {
///         BreadcrumbLink(on:click = move |_| ()) {"Home"}
///     }}}
/// }
/// # }
/// ```
///
/// There is no destination prop, because this framework has no notion of one: what a link does is
/// whatever its `on:click` does, and an application that routes hands it the same handler it hands
/// every other control that navigates.
#[component]
pub fn BreadcrumbLink(
    /// Classes merged after the link's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the entry reads as.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, BreadcrumbStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::Link));

    view! {
        control(
            class = "zui-breadcrumb__link",
            tabindex = {Focus::Sequential},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}

/// The last entry of the trail: the page the user is on.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Breadcrumb {BreadcrumbList {BreadcrumbItem {
///         BreadcrumbPage {"Billing"}
///     }}}
/// }
/// # }
/// ```
///
/// Not focusable and not operable, because a link to the page you are already on is a link that
/// does nothing — and announced as the *current* page, which is the whole reason the trail is
/// worth reading out at all.
#[component]
pub fn BreadcrumbPage(
    /// Classes merged after the entry's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the page is called.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, BreadcrumbStyle::CSS);
    let own = Attrs::new().a11y_from(
        A11yBinding::new(Role::Link)
            .current(AriaCurrent::Page)
            .disabled(true),
    );

    view! {
        box(class = "zui-breadcrumb__page", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// The punctuation between two entries.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Breadcrumb {BreadcrumbList {
///         BreadcrumbItem {BreadcrumbLink {"Home"}}
///         BreadcrumbSeparator()
///         BreadcrumbItem {BreadcrumbPage {"Billing"}}
///     }}
/// }
/// # }
/// ```
///
/// Hidden from a reader, and a chevron unless something else is written inside it.
#[component]
pub fn BreadcrumbSeparator(
    /// Classes merged after the separator's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What stands between the entries, when it is not the default chevron.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    install_stylesheet(SHEET, BreadcrumbStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::unspecified().hidden(true));
    let inside = match children {
        Some(children) => children.into_view_once(),
        None => AnyView::new(view! { Icon(icon = CHEVRON_RIGHT) }),
    };

    view! {
        box(class = "zui-breadcrumb__separator", {..own}, {..attrs}, class = class) {{inside}}
    }
}

/// A stand-in for the entries that were left out of a long trail.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Breadcrumb {BreadcrumbList {
///         BreadcrumbItem {BreadcrumbEllipsis()}
///     }}
/// }
/// # }
/// ```
///
/// It is announced, unlike the separators: entries were left out, and a reader who is not told
/// that hears a trail with a hole in it and no way to know there is one.
#[component]
pub fn BreadcrumbEllipsis(
    /// What the hidden entries are called, for a reader.
    #[prop(into, default = String::from("More"))]
    label: String,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, BreadcrumbStyle::CSS);

    view! {
        box(class = "zui-breadcrumb__ellipsis", {..attrs}, class = class) {
            Icon(icon = ELLIPSIS, label = label)
        }
    }
}
