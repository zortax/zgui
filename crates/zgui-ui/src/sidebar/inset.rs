//! The page beside the panel.

use zgui::prelude::*;
use zgui::{component, view};

use crate::sidebar::style;

/// The page beside the [`Sidebar`](crate::Sidebar).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {
///         Sidebar()
///         SidebarInset {SidebarTrigger()text {"The document"}}
///     }
/// }
/// # }
/// ```
///
/// Announced as the main region, which is the other half of what makes the panel a complementary
/// one: between them a reader is told which of the two is the thing they came for.
///
/// # Under the inset frame
///
/// With [`SidebarVariant::Inset`](crate::SidebarVariant::Inset) the page is the card: it is held
/// off every edge of the window, rounded and lifted, over a window tinted to the panel's own
/// colour. On the panel's side the gap is the panel's padding rather than the page's margin, until
/// the panel folds away and the page takes the gap over itself.
#[component]
pub fn SidebarInset(
    /// Classes merged after the page's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The page.
    children: Children,
) -> impl IntoView {
    style::install();
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::Main));

    view! {
        column(class = "zui-sidebar__inset", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
