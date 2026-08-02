//! A list of things to do, and everything that is one.
//!
//! A dropdown menu, a context menu and a submenu differ in exactly one thing: what opens them. What
//! they *are* — a portalled surface with one tab stop, arrow keys inside it, typeahead, items that
//! close the whole thing when chosen, and submenus that open on hover — is the same, and it is
//! written here once.
//!
//! ```no_run
//! use zgui::prelude::*;
//! use zgui::{component, view};
//! use zgui_ui::prelude::*;
//!
//! /// The actions on one row of a table.
//! #[component]
//! fn RowActions() -> impl IntoView {
//!     view! {
//!         DropdownMenu {
//!             DropdownMenuTrigger(variant = ButtonVariant::Ghost) {"…"}
//!             DropdownMenuContent {
//!                 MenuLabel {"Invoice"}
//!                 MenuItem {"Open"}
//!                 MenuItem {"Duplicate"}
//!                 MenuSeparator()
//!                 MenuSub {
//!                     MenuSubTrigger {"Export"}
//!                     MenuSubContent {
//!                         MenuItem {"PDF"}
//!                         MenuItem {"CSV"}
//!                     }
//!                 }
//!                 MenuSeparator()
//!                 MenuItem(destructive = true) {"Delete"}
//!             }
//!         }
//!     }
//! }
//! ```

mod check;
mod content;
mod item;
mod keys;
mod parts;
mod style;
mod sub;
mod typeahead;

pub use crate::menu::check::{
    MenuCheckboxItem, MenuCheckboxItemProps, MenuRadioContext, MenuRadioGroup, MenuRadioGroupProps,
    MenuRadioItem, MenuRadioItemProps,
};
pub use crate::menu::content::{MenuContent, MenuContentProps};
pub use crate::menu::item::{MenuItem, MenuItemProps};
pub use crate::menu::keys::{MenuTypeahead, MenuTypeaheadProps};
pub use crate::menu::parts::{
    MenuGroup, MenuGroupProps, MenuLabel, MenuLabelProps, MenuSeparator, MenuSeparatorProps,
    MenuShortcut, MenuShortcutProps,
};
pub use crate::menu::style::MenuStyle;
pub use crate::menu::sub::{
    CLOSE_DELAY, MenuSub, MenuSubContent, MenuSubContentProps, MenuSubProps, MenuSubTrigger,
    MenuSubTriggerProps, OPEN_DELAY, heading_toward,
};
pub use crate::menu::typeahead::{RESET_AFTER, Typeahead, matching};

// For the menubar's items, which take their highlight from the pointer the same way a menu's do.
pub(crate) use crate::menu::item::defocus_on_leave;

use zgui::prelude::*;

use crate::overlay::OverlayState;

/// What the menu's rules are installed under.
pub(crate) const SHEET: &str = "zui-menu";

/// The whole menu an item belongs to, however deep inside it that item is.
///
/// Choosing something four submenus down closes **all** of them, and nothing but the outermost
/// surface knows how to do that: [`OverlayState::current`] from inside a submenu answers with the
/// submenu. So the outermost one publishes itself here, submenus leave it alone, and an item asks
/// for it by name.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui::menu::MenuContext;
/// use zgui_ui::overlay::OverlayState;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let root = OverlayState::uncontrolled(true, None);
///     let menu = MenuContext::new(root).provide();
///     assert!(MenuContext::current().is_some());
///
///     menu.dismiss();
///     assert!(!root.is_open_untracked(), "choosing something closed the whole menu");
/// });
/// scope.unmount();
/// ```
#[derive(Copy, Clone)]
pub struct MenuContext {
    /// The outermost surface, which is what a chosen item closes.
    root: OverlayState,
}

impl MenuContext {
    /// Names `root` as the surface an item closes when it is chosen.
    #[must_use]
    pub const fn new(root: OverlayState) -> Self {
        Self { root }
    }

    /// Publishes this to every scope below the current one, and hands it back.
    pub fn provide(self) -> Self {
        provide_local_context(self);
        self
    }

    /// The menu the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The outermost surface.
    #[must_use]
    pub fn root(&self) -> OverlayState {
        self.root
    }

    /// Closes the whole menu, from wherever inside it this was called.
    pub fn dismiss(&self) {
        self.root.close();
    }
}
