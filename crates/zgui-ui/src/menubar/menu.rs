//! One menu of a bar: its name, its surface, and whether it is open.

use std::rc::Rc;

use zgui::prelude::*;
use zgui::{component, view};

use crate::menubar::style::MenubarStyle;
use crate::menubar::{MenubarContext, SHEET};

/// What a menu's name and its surface read to find each other.
#[derive(Clone)]
pub struct MenubarMenuContext {
    /// What this menu is called on its bar.
    value: Rc<String>,
    /// The name on the bar.
    trigger: NodeRef,
    /// The surface it opens.
    content: NodeRef,
    /// The bar it belongs to, when it is on one.
    bar: Option<MenubarContext>,
}

impl MenubarMenuContext {
    /// The menu the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// What this menu is called on its bar.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The name on the bar.
    #[must_use]
    pub fn trigger(&self) -> NodeRef {
        self.trigger
    }

    /// The surface it opens.
    #[must_use]
    pub fn content(&self) -> NodeRef {
        self.content
    }

    /// Whether this menu is the open one.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.bar.is_some_and(|bar| bar.is_open(&self.value))
    }

    /// Whether any menu on the same bar is open, this one or another.
    ///
    /// What a name on the bar asks when the focus arrives on it: with a menu already open, moving
    /// along the bar opens the one arrived at, and with nothing open it opens nothing.
    #[must_use]
    pub fn bar_has_something_open(&self) -> bool {
        self.bar.is_some_and(MenubarContext::any_open)
    }

    /// Opens this menu, closing whichever was open.
    pub fn open(&self) {
        if let Some(bar) = self.bar {
            bar.open(&self.value);
        }
    }

    /// Closes it.
    pub fn close(&self) {
        if let Some(bar) = self.bar {
            bar.close();
        }
    }

    /// Opens it if it was shut, and shuts it if it was open.
    pub fn toggle(&self) {
        if let Some(bar) = self.bar {
            bar.toggle(&self.value);
        }
    }

    /// Closes the menu and puts the focus back on the name that opened it.
    ///
    /// The pair, always: a menu that closed without moving the focus leaves the keyboard on an
    /// element that is no longer there, and the next <kbd>Tab</kbd> starts again from the top of
    /// the window.
    pub fn dismiss(&self) {
        self.close();
        self.trigger.focus();
    }
}

/// One menu of a [`Menubar`](crate::Menubar): a name on the bar and the surface it opens.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Menubar {
///         MenubarMenu(value = "file") {
///             MenubarTrigger {"File"}
///             MenubarContent {MenubarItem {"New"}}
///         }
///     }
/// }
/// # }
/// ```
///
/// Which menu is open belongs to the bar rather than to any menu, because opening one has to close
/// another and a menu that owned its own answer could only find out afterwards.
#[component]
pub fn MenubarMenu(
    /// What this menu is called on its bar.
    #[prop(into)]
    value: String,
    /// Classes merged after the menu's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The name and the surface.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);
    let context = MenubarMenuContext {
        value: Rc::new(value),
        trigger: NodeRef::new(),
        content: NodeRef::new(),
        bar: MenubarContext::current(),
    };
    provide_local_context(context.clone());

    let own = Attrs::new().attribute(zgui::view::AttrName::new("data-state"), {
        let context = context.clone();
        move || Some(if context.is_open() { "open" } else { "closed" }.to_owned())
    });

    view! {
        box(class = "zui-menubar__menu", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
