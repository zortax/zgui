//! A dialog that has to be answered rather than dismissed.

mod content;
mod parts;
mod style;

pub use crate::alert_dialog::content::{AlertDialogContent, AlertDialogContentProps};
pub use crate::alert_dialog::parts::{
    AlertDialogAction, AlertDialogActionProps, AlertDialogCancel, AlertDialogCancelProps,
    AlertDialogMedia, AlertDialogMediaProps, AlertDialogTrigger, AlertDialogTriggerProps,
};
pub use crate::alert_dialog::style::AlertDialogStyle;

/// What the alert dialog's own rules are installed under.
pub(crate) const SHEET: &str = "zui-alert-dialog";

/// How much room an alert dialog takes, which is decided by how much there is to read in it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum AlertDialogSize {
    /// Wide enough for a paragraph, with its answers ranged to the right.
    #[default]
    Default,
    /// Narrow, centred, and with its two answers sharing the width. For an interruption that is a
    /// picture, a line and a choice.
    Sm,
}

impl AlertDialogSize {
    /// Every size.
    pub const ALL: &'static [Self] = &[Self::Default, Self::Sm];

    /// How this is written as an attribute value, which is what the style sheet selects on.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
        }
    }
}

/// What is about to happen, spelled out — and what describes the surface to a reader.
pub use crate::dialog::DialogDescription as AlertDialogDescription;
/// The props of [`AlertDialogDescription`].
pub use crate::dialog::DialogDescriptionProps as AlertDialogDescriptionProps;
/// The row holding the answer and the way out of answering.
pub use crate::dialog::DialogFooter as AlertDialogFooter;
/// The props of [`AlertDialogFooter`].
pub use crate::dialog::DialogFooterProps as AlertDialogFooterProps;
/// The heading of an alert dialog. Laid out exactly as a [`DialogHeader`](crate::DialogHeader),
/// because it is one.
pub use crate::dialog::DialogHeader as AlertDialogHeader;
/// The props of [`AlertDialogHeader`].
pub use crate::dialog::DialogHeaderProps as AlertDialogHeaderProps;
/// What an alert dialog is asking, which is also what names it to a reader.
pub use crate::dialog::DialogTitle as AlertDialogTitle;
/// The props of [`AlertDialogTitle`].
pub use crate::dialog::DialogTitleProps as AlertDialogTitleProps;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::overlay::{OverlayState, SurfaceLabels};
use zgui_ui_primitives::Binding;

/// A modal surface that interrupts, and stays until one of its two answers is taken.
///
/// The difference from a [`Dialog`](crate::Dialog) is not how it looks. It is what counts as an
/// answer: a press on the dimmed window behind it does **not** close it, because a stray click is
/// not consent to delete a project. Escape still closes it — a keyboard user has to be able to get
/// out of something they opened by mistake, and Escape is unambiguous in a way a click past the
/// surface is not.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Deleting something, with a chance to change one's mind.
/// #[component]
/// fn DeleteProject() -> impl IntoView {
///     view! {
///         AlertDialog {
///             AlertDialogTrigger(variant = ButtonVariant::Destructive) {"Delete"}
///             AlertDialogContent {
///                 AlertDialogHeader {
///                     AlertDialogTitle {"Delete this project?"}
///                     AlertDialogDescription {
///                         "Its history goes with it, and none of it can be recovered."
///                     }
///                 }
///                 AlertDialogFooter {
///                     AlertDialogCancel {"Keep it"}
///                     AlertDialogAction(variant = ButtonVariant::Destructive) {"Delete"}
///                 }
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// <kbd>Escape</kbd> closes it. <kbd>Tab</kbd> cycles inside it, and focus goes back to whatever
/// opened it — which for a destructive action is the control the user is about to reconsider.
#[component]
pub fn AlertDialog(
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
