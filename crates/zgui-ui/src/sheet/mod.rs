//! A panel that slides in from one edge of the window.

mod content;
mod parts;
mod style;

pub use crate::sheet::content::{SheetContent, SheetContentProps, SheetDismiss, SheetDismissProps};
pub use crate::sheet::parts::{
    SheetDescription, SheetDescriptionProps, SheetFooter, SheetFooterProps, SheetHeader,
    SheetHeaderProps, SheetTitle, SheetTitleProps,
};
pub use crate::sheet::style::SheetStyle;

/// A control inside a sheet that closes it.
pub use crate::dialog::DialogClose as SheetClose;
/// The props of [`SheetClose`].
pub use crate::dialog::DialogCloseProps as SheetCloseProps;
/// What opens a sheet. A [`DialogTrigger`](crate::DialogTrigger), because that is what it is.
pub use crate::dialog::DialogTrigger as SheetTrigger;
/// The props of [`SheetTrigger`].
pub use crate::dialog::DialogTriggerProps as SheetTriggerProps;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::overlay::{OverlayState, SurfaceLabels};
use zgui_ui_primitives::Binding;

/// What the sheet's and drawer's rules are installed under.
pub(crate) const SHEET_NAME: &str = "zui-sheet";

/// Which edge a sheet comes in from.
///
/// One attribute rather than four components: the edge decides two things — which way the panel
/// slides and which of its borders it keeps — and both are style-sheet questions.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum SheetSide {
    /// Down from the top of the window.
    Top,
    /// In from the right, which is where a details panel goes.
    #[default]
    Right,
    /// Up from the bottom.
    Bottom,
    /// In from the left, which is where navigation goes.
    Left,
}

impl SheetSide {
    /// Every edge, clockwise from the top.
    pub const ALL: &'static [Self] = &[Self::Top, Self::Right, Self::Bottom, Self::Left];

    /// How this is written as an attribute value, which is what the style sheet selects on.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Left => "left",
        }
    }
}

/// A modal panel pinned to one edge of the window.
///
/// A sheet is a [`Dialog`](crate::Dialog) that comes from the side: the same scrim, the same focus
/// trap, the same return of focus to whatever opened it. What it buys is room — a settings panel
/// or a details pane that would be an awkward shape in a centred box is a natural one down the
/// side of the window.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// The details of whatever is selected, in a panel down the right.
/// #[component]
/// fn Details() -> impl IntoView {
///     view! {
///         Sheet {
///             SheetTrigger(variant = ButtonVariant::Outline) {"Details"}
///             SheetContent(side = SheetSide::Right) {
///                 SheetHeader {
///                     SheetTitle {"Invoice 4471"}
///                     SheetDescription {"Issued 3 March, due 17 March."}
///                 }
///                 SheetFooter {SheetClose {"Close"}}
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
pub fn Sheet(
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

#[cfg(test)]
mod tests {
    use super::SheetSide;

    #[test]
    fn every_edge_has_a_distinct_attribute_value() {
        let mut names: Vec<&str> = SheetSide::ALL.iter().map(|side| side.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SheetSide::ALL.len());
    }
}
