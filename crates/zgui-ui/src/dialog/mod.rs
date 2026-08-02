//! A surface that has to be answered before anything else can be.

mod content;
mod parts;
mod style;
mod trigger;

pub use crate::dialog::content::{
    DialogContent, DialogContentProps, DialogDismiss, DialogDismissProps,
};
pub use crate::dialog::parts::{
    DialogDescription, DialogDescriptionProps, DialogFooter, DialogFooterProps, DialogHeader,
    DialogHeaderProps, DialogTitle, DialogTitleProps,
};
pub use crate::dialog::style::DialogStyle;
pub use crate::dialog::trigger::{
    DialogClose, DialogCloseProps, DialogTrigger, DialogTriggerProps,
};

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::overlay::{OverlayState, SurfaceLabels};
use zgui_ui_primitives::Binding;

/// What the dialog's rules are installed under.
pub(crate) const SHEET: &str = "zui-dialog";

/// A modal surface, and everything that opens and closes it.
///
/// The root renders no element of its own. It owns whether the dialog is open and publishes that
/// to the trigger, the content and every close button inside it — which is what lets a dismiss
/// control three components deep close the dialog without a callback being threaded through
/// anything.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A confirmation, with somewhere to say yes.
/// #[component]
/// fn RenameProject() -> impl IntoView {
///     view! {
///         Dialog {
///             DialogTrigger {"Rename…"}
///             DialogContent {
///                 DialogHeader {
///                     DialogTitle {"Rename project"}
///                     DialogDescription {"Everyone on the team will see the new name."}
///                 }
///                 Input(placeholder = "Project name")
///                 DialogFooter {
///                     DialogClose(variant = ButtonVariant::Outline) {"Cancel"}
///                     Button {"Rename"}
///                 }
///             }
///         }
///     }
/// }
///
/// /// The same dialog, opened from somewhere else entirely.
/// #[component]
/// fn Controlled() -> impl IntoView {
///     let open = RwSignal::new_local(false);
///     view! {
///         box {
///             Button(on:click = move |_| open.set(true)) {"Rename…"}
///             Dialog(open = open, on_open_change = zgui::reactive::UnsyncCallback::new(move |next: bool| open.set(next))) {
///                 DialogContent {
///                     DialogTitle {"Rename project"}
///                 }
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// <kbd>Escape</kbd> closes it, and only it: a menu open inside the dialog takes the first press
/// and the dialog stays. <kbd>Tab</kbd> is confined to the surface while it is open, and focus goes
/// back to whatever opened it when it closes.
#[component]
pub fn Dialog(
    /// Whether it is open, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    open: Binding<bool>,
    /// Whether it starts open, when the dialog owns that itself.
    #[prop(default = false)]
    default_open: bool,
    /// Told whenever it opens or closes, whoever owns it.
    #[prop(optional)]
    on_open_change: Option<UnsyncCallback<bool>>,
    /// The trigger, the content, and anything else written beside them.
    children: Children,
) -> impl IntoView {
    OverlayState::new(open, default_open, on_open_change).provide();
    SurfaceLabels::provide();
    view! { {children.into_view_once()} }
}
