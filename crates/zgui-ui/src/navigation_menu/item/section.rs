//! One section of a navigation menu, and what its trigger and its panel read to find each other.

use std::rc::Rc;

use zgui::prelude::*;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};

use crate::navigation_menu::NavigationMenuContext;
use crate::navigation_menu::SHEET;
use crate::navigation_menu::style::NavigationMenuStyle;

/// What a section's trigger and its panel read to find each other.
#[derive(Clone)]
pub struct NavigationMenuItemContext {
    /// What this section is called.
    value: Rc<String>,
    /// The control that opens it.
    trigger: NodeRef,
    /// The panel it opens.
    content: NodeRef,
    /// The menu it belongs to, when it is in one.
    menu: Option<NavigationMenuContext>,
}

impl NavigationMenuItemContext {
    /// The section the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// What this section is called.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The control that opens it.
    #[must_use]
    pub fn trigger(&self) -> NodeRef {
        self.trigger
    }

    /// The panel it opens.
    #[must_use]
    pub fn content(&self) -> NodeRef {
        self.content
    }

    /// Whether this section is the open one.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.menu.is_some_and(|menu| menu.is_open(&self.value))
    }

    /// Opens this section if it was shut, and shuts it if it was open.
    pub fn toggle(&self) {
        if let Some(menu) = self.menu {
            menu.toggle(&self.value);
        }
    }

    /// Closes the section and leaves the focus where it is.
    ///
    /// What an outside press wants: the press has already given the focus somewhere else to be,
    /// and pulling it back onto the trigger would take it off the thing the user just chose.
    pub fn close(&self) {
        if let Some(menu) = self.menu {
            menu.close();
        }
    }

    /// Closes the section and puts the focus back on the control that opened it.
    pub fn dismiss(&self) {
        self.close();
        self.trigger.focus();
    }
}

/// One section of a [`NavigationMenuList`](crate::NavigationMenuList).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     NavigationMenu {NavigationMenuList {
///         NavigationMenuItem(value = "products") {
///             NavigationMenuTrigger {"Products"}
///             NavigationMenuContent {NavigationMenuLink {"Editor"}}
///         }
///     }}
/// }
/// # }
/// ```
///
/// A section with a panel takes a [`NavigationMenuTrigger`](crate::NavigationMenuTrigger); one that
/// is just a link takes a [`NavigationMenuLink`](crate::NavigationMenuLink) and no trigger at all.
/// Both are entries of the same list.
#[component]
pub fn NavigationMenuItem(
    /// What this section is called, which is what the menu reports as open.
    #[prop(into)]
    value: String,
    /// Classes merged after the section's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The trigger and the panel, or just a link.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, NavigationMenuStyle::CSS);
    let context = NavigationMenuItemContext {
        value: Rc::new(value),
        trigger: NodeRef::new(),
        content: NodeRef::new(),
        menu: NavigationMenuContext::current(),
    };
    provide_local_context(context.clone());

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), {
            let context = context.clone();
            move || Some(if context.is_open() { "open" } else { "closed" }.to_owned())
        })
        .a11y_from(A11yBinding::new(Role::ListItem));

    // Here rather than on the trigger, because a section is more than its trigger: a press on the
    // chevron, on the label, or on anything else an application puts in the section has to answer
    // the same way. The panel is portalled out of here and carries a listener of its own, for the
    // same reason and for the press this one can no longer see.
    let on_key = {
        let context = context.clone();
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            if matches!(ev.key, Key::Named(NamedKey::Escape)) && context.is_open() {
                context.dismiss();
                ev.prevent_default();
                ev.stop_propagation();
            }
        }
    };

    view! {
        box(class = "zui-navigation-menu__item", on:key_down = on_key, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
