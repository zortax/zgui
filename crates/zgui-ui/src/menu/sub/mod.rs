//! A menu that opens out of an item of another menu.

mod content;
mod triangle;
mod trigger;

pub use crate::menu::sub::content::{MenuSubContent, MenuSubContentProps};
pub use crate::menu::sub::triangle::heading_toward;
pub use crate::menu::sub::trigger::{MenuSubTrigger, MenuSubTriggerProps};

use core::time::Duration;

use zgui::prelude::*;
use zgui::{component, view};

use crate::overlay::{Delayed, OverlayState};
use zgui_ui_primitives::Binding;

/// How long the pointer rests on a submenu's trigger before it opens.
pub const OPEN_DELAY: Duration = Duration::from_millis(120);

/// How long a submenu stays open after the pointer leaves without heading for it.
pub const CLOSE_DELAY: Duration = Duration::from_millis(180);

/// The close a leaving pointer arms, shared between a submenu's trigger and its surface.
///
/// The trigger arms it when the pointer leaves without heading down the corridor, and takes it
/// back when the pointer returns. The surface has to be able to take it back too: a pointer that
/// crossed the corridor and *arrived* is not on its way anywhere, and a timer only the trigger
/// could reach would go on to close the submenu under a pointer that is inside it.
#[derive(Clone)]
pub(crate) struct SubIntent {
    /// The one armed close.
    closing: Delayed,
}

impl SubIntent {
    /// The intent of the submenu the calling scope is inside, when it is inside one.
    pub(crate) fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The armed close, to schedule or to take back.
    pub(crate) fn closing(&self) -> Delayed {
        self.closing.clone()
    }
}

/// A menu inside a menu: a trigger that is an item, and a surface beside it.
///
/// It is its own overlay — it opens, closes, dismisses and animates on the same machinery as every
/// other surface here — and it deliberately does **not** publish itself as the menu an item
/// closes. Choosing something three submenus deep closes the whole thing, and that only works
/// because the outermost menu is the one an item asks for.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A menu with a branch in it.
/// #[component]
/// fn Export() -> impl IntoView {
///     view! {
///         DropdownMenu {
///             DropdownMenuTrigger {"File"}
///             DropdownMenuContent {
///                 MenuItem {"Open"}
///                 MenuSub {
///                     MenuSubTrigger {"Export as"}
///                     MenuSubContent {
///                         MenuItem {"PDF"}
///                         MenuItem {"CSV"}
///                     }
///                 }
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// <kbd>→</kbd> and <kbd>Enter</kbd> on the trigger open the submenu and move into it;
/// <kbd>←</kbd> inside it closes it and returns to the trigger. <kbd>Escape</kbd> closes the whole
/// menu, because it belongs to the topmost open surface and that is the submenu's own layer.
///
/// # Pointer
///
/// It opens when the pointer rests on the trigger, and it does **not** close the moment the
/// pointer leaves: the path from the trigger to the submenu crosses the items below it, and a
/// submenu that closed on that path could only ever be reached by a perfect right angle. See
/// [`heading_toward`].
#[component]
pub fn MenuSub(
    /// Whether it is open, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    open: Binding<bool>,
    /// Whether it starts open, when it owns that itself.
    #[prop(default = false)]
    default_open: bool,
    /// Told whenever it opens or closes, whoever owns it.
    #[prop(optional)]
    on_open_change: Option<zgui::reactive::UnsyncCallback<bool>>,
    /// The trigger and the content.
    children: Children,
) -> impl IntoView {
    OverlayState::new(open, default_open, on_open_change).provide();
    // The close timer lives here, above both ends of it, because both ends have to reach the
    // same one: a trigger and a surface each holding their own could never cancel each other's.
    provide_local_context(SubIntent {
        closing: Delayed::new(),
    });
    view! { {children.into_view_once()} }
}
