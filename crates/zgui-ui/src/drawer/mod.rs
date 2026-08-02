//! A panel that comes up from the bottom of the window.

mod content;

pub use crate::drawer::content::{
    DrawerContent, DrawerContentProps, DrawerHandle, DrawerHandleProps,
};

/// A control inside a drawer that closes it.
pub use crate::dialog::DialogClose as DrawerClose;
/// The props of [`DrawerClose`].
pub use crate::dialog::DialogCloseProps as DrawerCloseProps;
/// What opens a drawer. A [`DialogTrigger`](crate::DialogTrigger), because that is what it is.
pub use crate::dialog::DialogTrigger as DrawerTrigger;
/// The props of [`DrawerTrigger`].
pub use crate::dialog::DialogTriggerProps as DrawerTriggerProps;
/// The line under a [`DrawerTitle`], which also describes the surface to a reader.
pub use crate::sheet::SheetDescription as DrawerDescription;
/// The props of [`DrawerDescription`].
pub use crate::sheet::SheetDescriptionProps as DrawerDescriptionProps;
/// The band of controls at the bottom of a drawer.
pub use crate::sheet::SheetFooter as DrawerFooter;
/// The props of [`DrawerFooter`].
pub use crate::sheet::SheetFooterProps as DrawerFooterProps;
/// The heading of a drawer. Centred, when the drawer came from an edge that runs the width of the
/// window.
pub use crate::sheet::SheetHeader as DrawerHeader;
/// The props of [`DrawerHeader`].
pub use crate::sheet::SheetHeaderProps as DrawerHeaderProps;
/// What a drawer is for, which is also what names it to a reader.
pub use crate::sheet::SheetTitle as DrawerTitle;
/// The props of [`DrawerTitle`].
pub use crate::sheet::SheetTitleProps as DrawerTitleProps;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::overlay::{OverlayState, SurfaceLabels};
use zgui_ui_primitives::Binding;

/// A modal panel that rises from the bottom edge, with a grab handle at its top.
///
/// The same machinery as a [`Sheet`](crate::Sheet) from the bottom, with two differences that are
/// the reason it is its own component: it keeps its top corners rounded, so it reads as something
/// pulled up rather than a wall that appeared, and it draws a [`DrawerHandle`] — the short bar that
/// says *this can be pulled*.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A short list of actions, up from the bottom.
/// #[component]
/// fn Share() -> impl IntoView {
///     view! {
///         Drawer {
///             DrawerTrigger(variant = ButtonVariant::Outline) {"Share"}
///             DrawerContent {
///                 DrawerHeader {
///                     DrawerTitle {"Share this invoice"}
///                     DrawerDescription {"Anyone with the link can read it."}
///                 }
///                 DrawerFooter {DrawerClose {"Done"}}
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// <kbd>Escape</kbd> closes it and <kbd>Tab</kbd> is confined to it, exactly as in a dialog.
#[component]
pub fn Drawer(
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
    on_open_change: Option<UnsyncCallback<bool>>,
    /// The trigger and the content.
    children: Children,
) -> impl IntoView {
    OverlayState::new(open, default_open, on_open_change).provide();
    SurfaceLabels::provide();
    view! { {children.into_view_once()} }
}
