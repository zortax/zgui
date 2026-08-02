//! The panel a context menu's items sit on.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};
use zgui_ui_primitives::{Align, Placement, Side};

use crate::menu::MenuContentProps;
use crate::overlay::OverlayState;

/// The items of a [`ContextMenu`](crate::ContextMenu), at the point it was asked for.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     ContextMenu {
///         ContextMenuTrigger {text {"A row"}}
///         ContextMenuContent {MenuItem {"Copy"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn ContextMenuContent(
    /// Where it is asked to go, before the window's edges have their say.
    ///
    /// Down and to the right of the point, which is what a context menu does — and the positioner
    /// crosses it to the other side on its own when there is no room there.
    #[prop(into, default = Signal::stored_local(Placement::new(Side::Bottom, Align::Start)))]
    placement: Signal<Placement, LocalStorage>,
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
        MenuContent(state = state, placement = placement, offset = {0.0}, {..attrs}, class = class) {
            {children.view()}
        }
    }
}
