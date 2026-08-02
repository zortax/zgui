//! Moving between the pages of something too long to show at once.

mod ends;
mod parts;
mod style;
mod window;

pub use crate::pagination::ends::{
    PaginationEllipsis, PaginationEllipsisProps, PaginationNext, PaginationNextProps,
    PaginationPrevious, PaginationPreviousProps,
};
pub use crate::pagination::parts::{
    PaginationContent, PaginationContentProps, PaginationItem, PaginationItemProps, PaginationLink,
    PaginationLinkProps,
};
pub use crate::pagination::style::PaginationStyle;
pub use crate::pagination::window::{Slot, page_window};

use zgui::prelude::*;
use zgui::{component, view};

/// What the pagination's rules are installed under.
pub(crate) const SHEET: &str = "zui-pagination";

/// A row of page numbers, with a way forward and a way back.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Twelve pages of results, seven slots to show them in.
/// #[component]
/// fn Results() -> impl IntoView {
///     let page = RwSignal::new_local(4_usize);
///     view! {
///         Pagination {
///             PaginationContent {
///                 PaginationItem {
///                     PaginationPrevious(on:click = move |_| page.update(|n| *n -= 1))
///                 }
///                 PaginationItem {
///                     PaginationLink(page = 4, current = true) {"4"}
///                 }
///                 PaginationItem {PaginationEllipsis()}
///                 PaginationItem {
///                     PaginationNext(on:click = move |_| page.update(|n| *n += 1))
///                 }
///             }
///         }
///     }
/// }
/// ```
///
/// # Which numbers to show
///
/// [`page_window`] answers that, and it is a plain function over three numbers rather than
/// anything this component does: given the page you are on, how many there are and how many slots
/// there is room for, it says which numbers and which gaps to draw. A caller who wants a different
/// rule writes a different function and this component is unchanged.
///
/// # What a reader is told
///
/// A navigation region, a list of entries, and — through [`PaginationLink`]'s `current` — which
/// number is the page being shown. That last one is the difference between a row of numbers and a
/// pager that can be operated without looking at it.
#[component]
pub fn Pagination(
    /// What the pager is called, for a reader.
    #[prop(into, default = String::from("Pagination"))]
    label: String,
    /// Classes merged after the pager's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The row of entries.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, PaginationStyle::CSS);
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-pagination"), true)
        .a11y_from(A11yBinding::new(Role::Navigation).label(label));

    view! {
        box(class = PaginationStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
