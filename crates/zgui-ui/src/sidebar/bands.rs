//! The bands the panel stacks: its head, its scrolling middle, its foot, and what divides them.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::Binding;

use crate::input::InputProps;
use crate::separator::SeparatorProps;
use crate::sidebar::style;

/// The band across the top of a [`Sidebar`](crate::Sidebar).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarHeader {text {"zgui"}}}}
/// }
/// # }
/// ```
#[component]
pub fn SidebarHeader(
    /// Classes merged after the band's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the band holds.
    children: Children,
) -> impl IntoView {
    style::install();
    view! {
        column(class = "zui-sidebar__header", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// The scrolling middle of a [`Sidebar`](crate::Sidebar).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {text {"Files"}}}}
/// }
/// # }
/// ```
///
/// The only band that scrolls, and the only one that clips: while the panel is folded to icons
/// there is no room for a scrollbar beside a column of icons, so it holds what it has instead.
#[component]
pub fn SidebarContent(
    /// Classes merged after the band's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The groups inside it.
    children: Children,
) -> impl IntoView {
    style::install();
    view! {
        column(class = "zui-sidebar__content", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// The band across the bottom of a [`Sidebar`](crate::Sidebar).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarFooter {text {"Signed in"}}}}
/// }
/// # }
/// ```
#[component]
pub fn SidebarFooter(
    /// Classes merged after the band's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the band holds.
    children: Children,
) -> impl IntoView {
    style::install();
    view! {
        column(class = "zui-sidebar__footer", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A rule across a [`Sidebar`](crate::Sidebar), between two runs of it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {
///         SidebarHeader {text {"zgui"}}
///         SidebarSeparator()
///         SidebarContent {text {"Files"}}
///     }}
/// }
/// # }
/// ```
///
/// Inset from both edges and drawn in the panel's own border colour, so it reads as part of the
/// panel rather than as the panel ending.
#[component]
pub fn SidebarSeparator(
    /// Classes merged after the rule's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    style::install();
    view! { Separator({..attrs}, class = "zui-sidebar__separator", class = class) }
}

/// A text field sized and coloured for a [`Sidebar`](crate::Sidebar).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarHeader {
///         SidebarInput(placeholder = "Search the project")
///     }}}
/// }
/// # }
/// ```
///
/// Shorter than an ordinary [`Input`](crate::Input) and flat rather than lifted: it sits *in* a
/// surface that is already off the page, and a field with its own lift on top of that reads as a
/// second card.
#[component]
pub fn SidebarInput(
    /// The text, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<String>,
    /// What it starts as, when the field owns it itself.
    #[prop(into, optional)]
    default_value: Option<String>,
    /// Told on every change, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// What to show while it is empty.
    #[prop(into, optional)]
    placeholder: Option<String>,
    /// Whether it can be typed into.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// The element whose text names this one.
    ///
    /// A field with nothing beside it wants `a11y:label` instead.
    #[prop(optional)]
    labelled_by: Option<NodeRef>,
    /// Where to record the field's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the field's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    style::install();
    view! {
        Input(
            value = value,
            default_value = default_value.unwrap_or_default(),
            on_change = on_change,
            placeholder = placeholder.unwrap_or_default(),
            disabled = disabled,
            labelled_by = labelled_by,
            node_ref = node_ref,
            {..attrs},
            class = "zui-sidebar__input",
            class = class
        )
    }
}
