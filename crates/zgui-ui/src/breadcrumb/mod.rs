//! The trail of where you are.

mod parts;
mod style;

pub use crate::breadcrumb::parts::{
    BreadcrumbEllipsis, BreadcrumbEllipsisProps, BreadcrumbItem, BreadcrumbItemProps,
    BreadcrumbLink, BreadcrumbLinkProps, BreadcrumbList, BreadcrumbListProps, BreadcrumbPage,
    BreadcrumbPageProps, BreadcrumbSeparator, BreadcrumbSeparatorProps,
};
pub use crate::breadcrumb::style::BreadcrumbStyle;

use zgui::prelude::*;
use zgui::{component, view};

/// What the breadcrumb's rules are installed under.
pub(crate) const SHEET: &str = "zui-breadcrumb";

/// The trail from the top of an interface down to where the user is.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Where in the settings we are.
/// #[component]
/// fn Trail() -> impl IntoView {
///     view! {
///         Breadcrumb {
///             BreadcrumbList {
///                 BreadcrumbItem {
///                     BreadcrumbLink(on:click = move |_| ()) {"Home"}
///                 }
///                 BreadcrumbSeparator()
///                 BreadcrumbItem {
///                     BreadcrumbLink(on:click = move |_| ()) {"Settings"}
///                 }
///                 BreadcrumbSeparator()
///                 BreadcrumbItem {BreadcrumbPage {"Billing"}}
///             }
///         }
///     }
/// }
/// ```
///
/// # What a reader is told
///
/// That the whole thing is a navigation region called *Breadcrumb*, that it is a list, and — the
/// part nothing else can say — *which* entry is the page you are on, through
/// [`BreadcrumbPage`]. Without that last one every crumb is announced the
/// same way and the trail says where you could go without ever saying where you are.
///
/// The separators are hidden from a reader outright. They are punctuation between the entries, and
/// a trail that reads out "slash" three times is a trail nobody listens to twice.
#[component]
pub fn Breadcrumb(
    /// What the trail is called, for a reader.
    #[prop(into, default = String::from("Breadcrumb"))]
    label: String,
    /// Classes merged after the trail's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The list.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, BreadcrumbStyle::CSS);
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-breadcrumb"), true)
        .a11y_from(A11yBinding::new(Role::Navigation).label(label));

    view! {
        box(class = BreadcrumbStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
