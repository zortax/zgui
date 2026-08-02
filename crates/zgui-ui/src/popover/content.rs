//! The surface a popover floats.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::ClassName;
use zgui::{component, view};
use zgui_ui_primitives::Placement;

use crate::overlay::{AnchoredSurfaceProps, OverlayState};
use crate::popover::SHEET;
use crate::popover::style::PopoverStyle;

/// The popover itself: a panel beside the trigger, on the side there is room on.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Popover {
///         PopoverTrigger {"Size"}
///         PopoverContent {Input(placeholder = "100%")}
///     }
/// }
/// # }
/// ```
///
/// # Which way it went
///
/// `data-side` and `data-align` land on the positioner around the panel, and say where it
/// *actually* ended up — which is not always where `placement` asked for, because the window's
/// edges have the last word. An arrow or a slide-in direction is written from those.
#[component]
pub fn PopoverContent(
    /// Where it is asked to go, before the window's edges have their say.
    #[prop(into, default = Signal::stored_local(Placement::BOTTOM))]
    placement: Signal<Placement, LocalStorage>,
    /// How far off the trigger it sits, in pixels.
    #[prop(default = 4.0)]
    offset: f32,
    /// Whether a press past it closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_outside_press: Signal<bool, LocalStorage>,
    /// Whether <kbd>Escape</kbd> closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_escape: Signal<bool, LocalStorage>,
    /// Classes merged after the popover's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the panel.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is on the panel.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, PopoverStyle::CSS);
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    let own = Attrs::new()
        .class_toggle(ClassName::new(PopoverStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-popover"), true);

    view! {
        AnchoredSurface(
            state = state,
            placement = placement,
            offset = offset,
            role = {Role::Dialog},
            trap = {FocusTrapOptions::MODAL},
            dismiss_on_outside_press = dismiss_on_outside_press,
            dismiss_on_escape = dismiss_on_escape,
            {..own},
            {..attrs},
            class = class
        ) {
            {children.view()}
        }
    }
}
