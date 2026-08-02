//! The surface a dialog puts its content on.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::ClassName;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::CROSS;

use crate::dialog::SHEET;
use crate::dialog::style::DialogStyle;
use crate::overlay::{ModalSurfaceProps, OverlayState, SurfaceLabels};

/// The dialog itself: a surface over a dimmed window, with focus confined to it.
///
/// Nothing here is mounted while the dialog is closed, so a dialog whose content is expensive
/// costs nothing until it is opened — and on the way out it stays mounted for exactly as long as
/// the style sheet's exit animation takes, and no longer.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Dialog {
///         DialogTrigger {"Open"}
///         DialogContent {
///             DialogHeader {DialogTitle {"Rename"}}
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn DialogContent(
    /// Whether a press on the dimmed window behind it closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_outside_press: Signal<bool, LocalStorage>,
    /// Whether <kbd>Escape</kbd> closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_escape: Signal<bool, LocalStorage>,
    /// Whether the surface draws a dismiss control in its own corner.
    #[prop(default = true)]
    dismiss_control: bool,
    /// Classes merged after the dialog's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the surface.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is on the surface.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, DialogStyle::CSS);
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    let labels = SurfaceLabels::current().unwrap_or_default();

    let own = Attrs::new()
        .class_toggle(ClassName::new(DialogStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-dialog"), true)
        .a11y_from(
            A11yBinding::unspecified()
                .labelled_by(labels.title())
                .described_by(labels.description()),
        );

    view! {
        ModalSurface(
            state = state,
            role = {Role::Dialog},
            dismiss_on_outside_press = dismiss_on_outside_press,
            dismiss_on_escape = dismiss_on_escape,
            {..own},
            {..attrs},
            class = class
        ) {
            {children.view()}
            if move || dismiss_control {
                DialogDismiss()
            } else {}
        }
    }
}

/// The cross in a dialog's own corner.
///
/// Its own element rather than a [`DialogClose`](crate::DialogClose) with a class, because it is
/// positioned against the surface rather than laid out in it: a dialog with no footer and no
/// header still has somewhere to put it.
///
/// [`DialogContent`] draws one unless told not to, and this is what a surface of one's own writes
/// to get the same control in the same place.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Dialog {
///         DialogContent(dismiss_control = false) {
///             DialogTitle {"Rename"}
///             DialogDismiss()
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn DialogDismiss(
    /// Anything the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    let state = OverlayState::current();
    view! {
        control(
            class = "zui-dialog__dismiss",
            tabindex = {Focus::Sequential},
            a11y:role = {Role::Button},
            a11y:label = "Close",
            on:click = move |_| {
                if let Some(state) = state {
                    state.close();
                }
            },
            {..attrs}
        ) {
            Icon(icon = CROSS)
        }
    }
}
