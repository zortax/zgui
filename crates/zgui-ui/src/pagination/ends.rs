//! The way forward, the way back, and the gap between runs of numbers.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::{CHEVRON_LEFT, CHEVRON_RIGHT};
use zgui_ui_icons::set::ui::ELLIPSIS;

use crate::pagination::SHEET;
use crate::pagination::style::PaginationStyle;

/// The control that goes back one page.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Pagination {PaginationContent {PaginationItem {
///         PaginationPrevious(disabled = true)
///     }}}
/// }
/// # }
/// ```
///
/// Disabled on the first page rather than taken away, so the row does not change width under the
/// pointer as a user pages towards the front of a list.
///
/// The word beside the arrow is what tells a hurried reader which way it goes; the arrow alone is
/// the same shape in both directions at a glance.
#[component]
pub fn PaginationPrevious(
    /// Whether there is a page to go back to.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// What it is called, for a reader.
    #[prop(into, default = String::from("Previous page"))]
    label: String,
    /// The word beside the arrow.
    #[prop(into, default = String::from("Previous"))]
    text: String,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, PaginationStyle::CSS);
    let own = Attrs::new()
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(
            A11yBinding::new(Role::Link)
                .label(label)
                .disabled(move || disabled.get()),
        );

    view! {
        control(
            class = "zui-pagination__link",
            class = "zui-pagination__previous",
            tabindex = {Focus::Sequential},
            {..own},
            {..attrs},
            class = class
        ) {
            Icon(icon = CHEVRON_LEFT)
            label {{text}}
        }
    }
}

/// The control that goes on one page.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Pagination {PaginationContent {PaginationItem {
///         PaginationNext()
///     }}}
/// }
/// # }
/// ```
#[component]
pub fn PaginationNext(
    /// Whether there is a page to go on to.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// What it is called, for a reader.
    #[prop(into, default = String::from("Next page"))]
    label: String,
    /// The word beside the arrow.
    #[prop(into, default = String::from("Next"))]
    text: String,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, PaginationStyle::CSS);
    let own = Attrs::new()
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(
            A11yBinding::new(Role::Link)
                .label(label)
                .disabled(move || disabled.get()),
        );

    view! {
        control(
            class = "zui-pagination__link",
            class = "zui-pagination__next",
            tabindex = {Focus::Sequential},
            {..own},
            {..attrs},
            class = class
        ) {
            label {{text}}
            Icon(icon = CHEVRON_RIGHT)
        }
    }
}

/// The gap where a run of page numbers was left out.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Pagination {PaginationContent {PaginationItem {
///         PaginationEllipsis()
///     }}}
/// }
/// # }
/// ```
///
/// Announced rather than hidden, because pages were left out and a reader who is not told that
/// hears a pager that skips from 2 to 17 for no reason. Which numbers a gap stands for is
/// [`page_window`](crate::pagination::page_window)'s answer, not this component's.
#[component]
pub fn PaginationEllipsis(
    /// What the left-out pages are called, for a reader.
    #[prop(into, default = String::from("More pages"))]
    label: String,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, PaginationStyle::CSS);

    view! {
        box(class = "zui-pagination__ellipsis", {..attrs}, class = class) {
            Icon(icon = ELLIPSIS, label = label)
        }
    }
}
