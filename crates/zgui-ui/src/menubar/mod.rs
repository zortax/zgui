//! A bar of menus across the top of a window.

mod arrows;
mod choice;
mod content;
mod item;
mod menu;
mod style;
mod trigger;

/// A run of related menubar items under one name, for a reader.
pub use crate::menu::MenuGroup as MenubarGroup;
/// The props of [`MenubarGroup`].
pub use crate::menu::MenuGroupProps as MenubarGroupProps;
/// A run of menubar items of which exactly one is chosen. A
/// [`MenuRadioGroup`](crate::MenuRadioGroup), because what a group of choices *is* does not change
/// with the surface it is on.
pub use crate::menu::MenuRadioGroup as MenubarRadioGroup;
/// The props of [`MenubarRadioGroup`].
pub use crate::menu::MenuRadioGroupProps as MenubarRadioGroupProps;
pub use crate::menubar::arrows::{MenubarArrows, MenubarArrowsProps};
pub use crate::menubar::choice::{
    MenubarCheckboxItem, MenubarCheckboxItemProps, MenubarRadioItem, MenubarRadioItemProps,
};
pub use crate::menubar::content::{MenubarContent, MenubarContentProps};
pub use crate::menubar::item::{
    MenubarItem, MenubarItemProps, MenubarLabel, MenubarLabelProps, MenubarSeparator,
    MenubarSeparatorProps, MenubarShortcut, MenubarShortcutProps,
};
pub use crate::menubar::menu::{MenubarMenu, MenubarMenuContext, MenubarMenuProps};
pub use crate::menubar::style::MenubarStyle;
pub use crate::menubar::trigger::{MenubarTrigger, MenubarTriggerProps};

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::{component, view};
use zgui_ui_primitives::Orientation;
use zgui_ui_primitives::prelude::*;

/// What the menubar's rules are installed under.
pub(crate) const SHEET: &str = "zui-menubar";

/// Which of a bar's menus is open, and what opening another one means.
#[derive(Copy, Clone)]
pub struct MenubarContext {
    /// The menu that is open, by name, when one is.
    open: RwSignal<Option<String>, LocalStorage>,
}

impl MenubarContext {
    /// The bar the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Whether the menu called `value` is the open one.
    #[must_use]
    pub fn is_open(self, value: &str) -> bool {
        self.open.with(|open| open.as_deref() == Some(value))
    }

    /// Whether any menu on the bar is open.
    #[must_use]
    pub fn any_open(self) -> bool {
        self.open.with(Option::is_some)
    }

    /// Opens the menu called `value`, closing whichever was open.
    pub fn open(self, value: &str) {
        if !self.is_open(value) {
            self.open.set(Some(value.to_owned()));
        }
    }

    /// Closes whichever menu is open.
    pub fn close(self) {
        if self.open.get_untracked().is_some() {
            self.open.set(None);
        }
    }

    /// Opens the menu called `value` if it was shut, and shuts it if it was open.
    pub fn toggle(self, value: &str) {
        if self.is_open(value) {
            self.close();
        } else {
            self.open(value);
        }
    }
}

/// A bar of menus, of the kind that runs across the top of an application.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// The two menus of a very small editor.
/// #[component]
/// fn Bar() -> impl IntoView {
///     view! {
///         Menubar(label = "Main menu") {
///             MenubarMenu(value = "file") {
///                 MenubarTrigger {"File"}
///                 MenubarContent {
///                     MenubarItem {"New"}
///                     MenubarSeparator()
///                     MenubarItem {"Quit"}
///                 }
///             }
///             MenubarMenu(value = "edit") {
///                 MenubarTrigger {"Edit"}
///                 MenubarContent {MenubarItem {"Undo"}}
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// One tab stop for the whole bar. <kbd>←</kbd> and <kbd>→</kbd> move between the menu names;
/// <kbd>↓</kbd>, <kbd>Enter</kbd> and <kbd>Space</kbd> open the one they are on and put the focus
/// on its first item; inside a menu <kbd>↑</kbd> and <kbd>↓</kbd> move between the items,
/// <kbd>Home</kbd> and <kbd>End</kbd> go to the ends, typing a letter moves to the next item that
/// reads as beginning with it, and <kbd>Escape</kbd> closes the menu and puts the focus back on the
/// name it came from.
///
/// The bar answers only the horizontal arrows and each menu only the vertical ones, which is what
/// makes the two nest at all: a bar that answered <kbd>↓</kbd> would swallow the key that enters
/// the menu it just opened.
///
/// # Moving between menus with one already open
///
/// Arrowing along the bar while a menu is open opens the next one as it is reached, which is the
/// behaviour every desktop menubar has and the reason a bar is worth having over a row of
/// unrelated dropdowns.
#[component]
pub fn Menubar(
    /// What the bar is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the bar's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The menus.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);
    let context = MenubarContext {
        open: RwSignal::new_local(None),
    };
    provide_local_context(context);

    let mut semantics =
        A11yBinding::new(Role::MenuBar).orientation(zgui::vocab::Orientation::Horizontal);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-menubar"), true)
        .class_toggle(zgui::view::ClassName::new(MenubarStyle::CLASS), true)
        .a11y_from(semantics);

    view! {
        RovingFocus(orientation = Orientation::Horizontal, class = class, {..own}, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
