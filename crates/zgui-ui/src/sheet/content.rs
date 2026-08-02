//! The panel a sheet slides in.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::{AttrName, ClassName};
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::CROSS;

use crate::overlay::{ModalSurfaceProps, OverlayState, SurfaceLabels};
use crate::sheet::style::SheetStyle;
use crate::sheet::{SHEET_NAME, SheetSide};

/// The sheet itself: a panel pinned to one edge, over a dimmed window.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Sheet {
///         SheetTrigger {"Open"}
///         SheetContent(side = SheetSide::Left) {
///             SheetTitle {"Navigation"}
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn SheetContent(
    /// Which edge it comes in from.
    #[prop(default = SheetSide::Right)]
    side: SheetSide,
    /// Whether a press on the dimmed window behind it closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_outside_press: Signal<bool, LocalStorage>,
    /// Whether <kbd>Escape</kbd> closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_escape: Signal<bool, LocalStorage>,
    /// Whether the panel draws a dismiss control in its own corner.
    #[prop(default = true)]
    dismiss_control: bool,
    /// Classes merged after the panel's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the panel.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is on the panel.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET_NAME, SheetStyle::CSS);
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    let labels = SurfaceLabels::current().unwrap_or_default();

    let own = Attrs::new()
        .class_toggle(ClassName::new(SheetStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-sheet"), true)
        .attribute(AttrName::new("data-side"), side.name())
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
                SheetDismiss()
            } else {}
        }
    }
}

/// The cross in a sheet's own corner.
///
/// Positioned against the panel rather than laid out in one of its bands, so a sheet with no
/// header still has one and it is in the same place either way.
///
/// [`SheetContent`] draws one unless told not to, and this is what a panel of one's own writes to
/// get the same control in the same corner.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::sheet::{SheetDismiss, SheetDismissProps};
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Sheet {
///         SheetContent(dismiss_control = false) {
///             SheetTitle {"Details"}
///             SheetDismiss()
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn SheetDismiss(
    /// Anything the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET_NAME, SheetStyle::CSS);
    let state = OverlayState::current();
    view! {
        control(
            class = "zui-sheet__dismiss",
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
