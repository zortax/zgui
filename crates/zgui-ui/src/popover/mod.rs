//! A small surface that floats beside the control that opened it.

mod content;
mod parts;
mod style;

pub use crate::popover::content::{PopoverContent, PopoverContentProps};
pub use crate::popover::parts::{
    PopoverDescription, PopoverDescriptionProps, PopoverHeader, PopoverHeaderProps, PopoverTitle,
    PopoverTitleProps,
};
pub use crate::popover::style::PopoverStyle;

/// A control inside a popover that closes it.
pub use crate::dialog::DialogClose as PopoverClose;
/// The props of [`PopoverClose`].
pub use crate::dialog::DialogCloseProps as PopoverCloseProps;
/// What opens a popover. A [`DialogTrigger`](crate::DialogTrigger), because that is what it is.
pub use crate::dialog::DialogTrigger as PopoverTrigger;
/// The props of [`PopoverTrigger`].
pub use crate::dialog::DialogTriggerProps as PopoverTriggerProps;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::overlay::OverlayState;
use zgui_ui_primitives::Binding;

/// What the popover's rules are installed under.
pub(crate) const SHEET: &str = "zui-popover";

/// A surface anchored to a control, holding whatever will not fit beside it.
///
/// The difference from a [`Dialog`](crate::Dialog) is what it costs the reader: a popover does not
/// dim the window, does not stop it scrolling, and is anchored to the thing it belongs to rather
/// than to the middle of the screen. It still confines the keyboard while it is open and gives
/// focus back on the way out, because a surface with controls in it that the caret never reaches
/// is a surface a keyboard user cannot use.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui_primitives::{Align, Placement, Side};
///
/// /// A quick way to change one setting, without leaving the page.
/// #[component]
/// fn Dimensions() -> impl IntoView {
///     view! {
///         Popover {
///             PopoverTrigger(variant = ButtonVariant::Outline) {"Size"}
///             PopoverContent(placement = Placement::new(Side::Bottom, Align::Start)) {
///                 Label {"Width"}
///                 Input(placeholder = "100%")
///                 PopoverClose {"Done"}
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// <kbd>Escape</kbd> closes it and returns focus to the trigger. <kbd>Tab</kbd> cycles inside it
/// while it is open.
#[component]
pub fn Popover(
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
    view! { {children.into_view_once()} }
}
