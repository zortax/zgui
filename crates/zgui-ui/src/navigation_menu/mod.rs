//! A bar of sections that open a panel underneath.

mod item;
mod link;
mod style;

pub use crate::navigation_menu::item::{
    NavigationMenuContent, NavigationMenuContentProps, NavigationMenuIndicator,
    NavigationMenuIndicatorProps, NavigationMenuItem, NavigationMenuItemContext,
    NavigationMenuItemProps, NavigationMenuList, NavigationMenuListProps, NavigationMenuTrigger,
    NavigationMenuTriggerProps,
};
pub use crate::navigation_menu::link::{NavigationMenuLink, NavigationMenuLinkProps};
pub use crate::navigation_menu::style::NavigationMenuStyle;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use zgui_ui_primitives::{Binding, Controllable};

/// What the navigation menu's rules are installed under.
pub(crate) const SHEET: &str = "zui-navigation-menu";

/// Which section of a navigation menu is open.
#[derive(Copy, Clone)]
pub struct NavigationMenuContext {
    /// The section that is open, by name; empty when none is.
    value: Controllable<String>,
}

impl NavigationMenuContext {
    /// The menu the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Whether the section called `value` is the open one.
    #[must_use]
    pub fn is_open(self, value: &str) -> bool {
        self.value.get() == value
    }

    /// Whether any section is open.
    #[must_use]
    pub fn any_open(self) -> bool {
        !self.value.get().is_empty()
    }

    /// Opens the section called `value`, closing whichever was open.
    pub fn open(self, value: &str) {
        self.value.set(value.to_owned());
    }

    /// Closes whichever section is open.
    pub fn close(self) {
        self.value.set(String::new());
    }

    /// Opens the section called `value` if it was shut, and shuts it if it was open.
    pub fn toggle(self, value: &str) {
        if self.is_open(value) {
            self.close();
        } else {
            self.open(value);
        }
    }
}

/// A bar of sections, each opening a panel of links underneath it.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Two sections of a site's navigation.
/// #[component]
/// fn SiteNav() -> impl IntoView {
///     view! {
///         NavigationMenu(label = "Main") {
///             NavigationMenuList {
///                 NavigationMenuItem(value = "products") {
///                     NavigationMenuTrigger {"Products"}
///                     NavigationMenuContent {
///                         NavigationMenuLink {"Editor"}
///                         NavigationMenuLink(active = true) {"Runtime"}
///                     }
///                 }
///                 NavigationMenuItem(value = "pricing") {
///                     NavigationMenuLink {"Pricing"}
///                 }
///             }
///         }
///     }
/// }
/// ```
///
/// # A navigation menu is not a menubar
///
/// It is a list of links, and it is announced as one: the sections are disclosure buttons over
/// panels of links rather than menu items, because a reader who is told "menu" expects the menu
/// keyboard model — and a site's navigation is not that. What it *is* is
/// [`Role::Navigation`] around a list, with each open section a
/// button that says whether it is expanded and what it controls.
///
/// # What moves
///
/// The panel arrives from slightly under its own size and fades in over two tenths of a second,
/// leaving the same way; the chevron on its trigger turns half a circle over three tenths, which is
/// slower because it is the part a reader watches rather than reads. A section may also carry a
/// [`NavigationMenuIndicator`], an arrow between the bar and the panel that fades with it.
///
/// # Keyboard
///
/// One tab stop for the bar; <kbd>←</kbd> and <kbd>→</kbd> move between the sections;
/// <kbd>Enter</kbd> and <kbd>Space</kbd> open the one they are on; <kbd>Escape</kbd> closes it. The
/// links inside an open panel are ordinary tab stops.
#[component]
pub fn NavigationMenu(
    /// Which section is open, when the caller holds it. An empty name means none.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<String>,
    /// Which one starts open, when the menu owns that itself.
    #[prop(into, default = String::new())]
    default_value: String,
    /// Told whenever the open section changes, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// What the whole menu is called, for a reader.
    #[prop(into, default = String::from("Main"))]
    label: String,
    /// Where to record this component's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the menu's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The list of sections.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, NavigationMenuStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let context = NavigationMenuContext {
        value: Controllable::new(value, default_value, on_change),
    };
    provide_local_context(context);

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-navigation-menu"), true)
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if context.any_open() { "open" } else { "closed" }.to_owned())
        })
        .a11y_from(A11yBinding::new(Role::Navigation).label(label));

    view! {
        box(node_ref = element, class = NavigationMenuStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
