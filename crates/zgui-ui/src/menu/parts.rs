//! The pieces of a menu that are not commands.

use zgui::prelude::*;
use zgui::{component, view};

use crate::menu::SHEET;
use crate::menu::style::MenuStyle;

/// A heading over a run of items, saying what they are about.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     DropdownMenu {DropdownMenuContent {
///         MenuLabel {"Invoice"}
///         MenuItem {"Open"}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn MenuLabel(
    /// Whether it is indented to the column the ticks and bullets leave for a label.
    #[prop(default = false)]
    inset: bool,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The heading.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenuStyle::CSS);
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-menu__label"), true)
        .class_toggle(zgui::view::ClassName::new("zui-menu__label--inset"), inset);
    view! {
        label({..own}, {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// A rule between two runs of items.
///
/// Decorative, and declared so. A reader that met it would be told there is a horizontal line
/// between two things it has already been told apart by the group they are in.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     DropdownMenu {DropdownMenuContent {
///         MenuItem {"Open"}
///         MenuSeparator()
///         MenuItem {"Delete"}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn MenuSeparator(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, MenuStyle::CSS);
    view! { box(class = "zui-menu__separator", a11y:hidden = true, {..attrs}, class = class) }
}

/// A run of items that belong together, announced as a group.
///
/// The difference from writing the items loose: a reader is told *three items, Export* rather than
/// three items in a list of eleven.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     DropdownMenu {DropdownMenuContent {
///         MenuGroup(label = "Export") {
///             MenuItem {"PDF"}
///             MenuItem {"CSV"}
///         }
///     }}
/// }
/// # }
/// ```
#[component]
pub fn MenuGroup(
    /// What the group is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenuStyle::CSS);
    let mut semantics = A11yBinding::new(Role::Group);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-menu__group"), true)
        .a11y_from(semantics);
    view! { box({..own}, {..attrs}, class = class) {{children.into_view_once()}} }
}

/// The keystroke that does the same thing as an item, written at its right-hand end.
///
/// It is a picture of the keystroke and is kept out of the accessibility tree, because a reader is
/// told about a shortcut by the item itself: [`MenuItem`](crate::MenuItem)'s `shortcut` prop draws
/// one of these *and* declares it, and writing this by hand is for a layout that needs the mark
/// somewhere else.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     DropdownMenu {DropdownMenuContent {
///         MenuItem(shortcut = "⌘O") {"Open"}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn MenuShortcut(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The keystroke.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenuStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::unspecified().hidden(true));
    view! {
        text(class = "zui-menu__shortcut", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
