//! The panel a dropdown menu's items sit on.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};
use zgui_ui_primitives::{Align, Placement, Side};

use crate::menu::MenuContentProps;
use crate::overlay::OverlayState;

/// The items of a [`DropdownMenu`](crate::DropdownMenu), under its trigger.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     DropdownMenu {
///         DropdownMenuTrigger {"Account"}
///         DropdownMenuContent {MenuItem {"Settings"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn DropdownMenuContent(
    /// Where it is asked to go, before the window's edges have their say.
    #[prop(into, default = Signal::stored_local(Placement::new(Side::Bottom, Align::Start)))]
    placement: Signal<Placement, LocalStorage>,
    /// How far off the trigger it sits, in pixels.
    #[prop(default = 4.0)]
    offset: f32,
    /// Classes merged after the menu's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the panel.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: ChildrenFn,
) -> impl IntoView {
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    view! {
        MenuContent(state = state, placement = placement, offset = offset, {..attrs}, class = class) {
            {children.view()}
        }
    }
}
