//! The row a pager is written out of.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::{AriaCurrent, UiState};
use zgui::{component, view};

use crate::pagination::SHEET;
use crate::pagination::style::PaginationStyle;

/// The row of entries inside a [`Pagination`](crate::Pagination).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Pagination {PaginationContent {
///         PaginationItem {PaginationLink(page = 1, current = true) {"1"}}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn PaginationContent(
    /// Classes merged after the row's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The entries.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, PaginationStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::List));

    view! {
        row(class = "zui-pagination__content", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// One entry of a [`PaginationContent`].
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Pagination {PaginationContent {
///         PaginationItem {PaginationLink(page = 2) {"2"}}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn PaginationItem(
    /// Classes merged after the entry's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the entry holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, PaginationStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::ListItem));

    view! {
        box(class = "zui-pagination__item", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// One page number, which can be gone to.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Pagination {PaginationContent {PaginationItem {
///         PaginationLink(page = 4, current = true) {"4"}
///     }}}
/// }
/// # }
/// ```
///
/// The page you are on is announced as the current page and carries `data-current`, so a sheet
/// picks it out and a reader is told which of a dozen identical-sounding numbers is where you are.
#[component]
pub fn PaginationLink(
    /// Which page this goes to, which is what a reader is told it is called.
    #[prop(optional)]
    page: Option<usize>,
    /// Whether this is the page being shown.
    #[prop(into, default = Signal::stored_local(false))]
    current: Signal<bool, LocalStorage>,
    /// Classes merged after the link's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the entry reads as, which is usually the number.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, PaginationStyle::CSS);
    let mut semantics = A11yBinding::new(Role::Link)
        .current(move || {
            if current.get() {
                AriaCurrent::Page
            } else {
                AriaCurrent::False
            }
        })
        .selected(move || current.get());
    if let Some(page) = page {
        semantics = semantics.label(format!("Page {page}"));
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-current"), move || {
            current.get().then(|| "true".to_owned())
        })
        .state(UiState::CHECKED, move || current.get())
        .a11y_from(semantics);

    view! {
        control(
            class = "zui-pagination__link",
            tabindex = {Focus::Sequential},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
