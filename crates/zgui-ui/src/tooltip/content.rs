//! The label a tooltip shows.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::ClassName;
use zgui::{component, view};
use zgui_ui_primitives::Placement;

use crate::overlay::{AnchoredSurfaceProps, HoverIntent, OverlayState};
use crate::tooltip::SHEET;
use crate::tooltip::arrow::TooltipArrowProps;
use crate::tooltip::style::TooltipStyle;

/// What the tooltip says.
///
/// It stays up while the pointer is on it, so a tooltip long enough to need reading can be read.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Tooltip {
///         TooltipTrigger {Button {"?"}}
///         TooltipContent {"Applies to this account only"}
///     }
/// }
/// # }
/// ```
#[component]
pub fn TooltipContent(
    /// Where it is asked to go, before the window's edges have their say.
    #[prop(into, default = Signal::stored_local(Placement::TOP))]
    placement: Signal<Placement, LocalStorage>,
    /// How far off the trigger it sits, in pixels.
    ///
    /// Flush against it, so that the arrow reaches the control rather than pointing at a gap.
    #[prop(default = 0.0)]
    offset: f32,
    /// Whether it draws the arrow on the edge facing its trigger.
    #[prop(default = true)]
    arrow: bool,
    /// Classes merged after the tooltip's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the label.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, TooltipStyle::CSS);
    let intent = HoverIntent::current();
    let state = intent.as_ref().map_or_else(
        || OverlayState::uncontrolled(false, None),
        HoverIntent::state,
    );

    let on_enter = intent.clone();
    let on_leave = intent.clone();
    let own = Attrs::new()
        .class_toggle(ClassName::new(TooltipStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-tooltip"), true)
        .listener(
            events::POINTER_ENTER,
            zgui::vocab::ListenerOptions::DEFAULT,
            move |_: &mut EventCx<'_, events::PointerEnter>| {
                if let Some(intent) = &on_enter {
                    intent.enter();
                }
            },
        )
        .listener(
            events::POINTER_LEAVE,
            zgui::vocab::ListenerOptions::DEFAULT,
            move |_: &mut EventCx<'_, events::PointerLeave>| {
                if let Some(intent) = &on_leave {
                    intent.leave();
                }
            },
        );

    view! {
        AnchoredSurface(
            state = state,
            placement = placement,
            offset = offset,
            role = {Role::Tooltip},
            // A tooltip takes no focus and holds nothing to operate, so there is nothing to
            // confine — and a trap here would take the caret off the control being described.
            dismiss_on_outside_press = {false},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.view()}
            if move || arrow {
                TooltipArrow()
            } else {}
        }
    }
}
