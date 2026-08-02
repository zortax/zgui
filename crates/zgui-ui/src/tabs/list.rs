//! The strip of tabs.

use zgui::prelude::*;
use zgui::{component, variants, view};
use zgui_ui_primitives::prelude::*;

use crate::support::variant_attrs;
use crate::tabs::style::TabsStyle;
use crate::tabs::{SHEET, TabsContext};

variants! {
    /// The axes a [`TabsList`] varies along.
    pub TabsListVariants {
        base: "zui-tabs__list",
        variant: {
            Default => "zui-tabs__list--default",
            Line => "zui-tabs__list--line",
        } = Default,
    }
}

/// The strip of tabs across the top — or down the side — of a [`Tabs`](crate::Tabs).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Tabs(default_value = "profile") {
///         TabsList {
///             TabsTrigger(value = "profile") {"Profile"}
///         }
///         TabsContent(value = "profile") {text {"Your name."}}
///     }
/// }
/// # }
/// ```
///
/// # The two strips
///
/// The default one is a trough with the chosen tab raised out of it as a pill. The lined one has no
/// trough: the tabs sit on the page and the chosen one is marked by a rule along the strip's far
/// edge, which fades between tabs rather than sliding.
///
/// The strip is one tab stop and the arrow keys move within it, which is
/// [`RovingFocus`](zgui_ui_primitives::RovingFocus) doing the work — the same primitive a toolbar
/// and a radio group are built on. Its orientation comes from the enclosing tab set, so a vertical
/// strip answers <kbd>↑</kbd> and <kbd>↓</kbd> and leaves <kbd>←</kbd> and <kbd>→</kbd> alone.
#[component]
pub fn TabsList(
    /// Which of the two strips this is.
    #[prop(default = TabsListVariant::Default)]
    variant: TabsListVariant,
    /// What the strip is called, for a reader, when the tab set itself is not named.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the strip's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The tabs.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, TabsStyle::CSS);
    let context = TabsContext::current();
    let orientation = context.map_or(Orientation::Horizontal, TabsContext::orientation);

    let mut semantics = A11yBinding::new(Role::TabList).orientation(match orientation {
        Orientation::Vertical => zgui::vocab::Orientation::Vertical,
        _ => zgui::vocab::Orientation::Horizontal,
    });
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let variants = TabsListVariants { variant };
    let own = variant_attrs(variants.classes(), variants.data_attributes()).a11y_from(semantics);

    view! {
        RovingFocus(orientation = orientation, class = class, {..own}, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
