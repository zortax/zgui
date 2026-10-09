//! The label a tooltip shows.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::{AttrName, ClassName};
use zgui::{component, view};
use zgui_ui_primitives::Placement;

use crate::overlay::{AnchoredSurfaceProps, HoverIntent, OverlayState};
use crate::tooltip::SHEET;
use crate::tooltip::arrow::TooltipArrowProps;
use crate::tooltip::geom::side_offset;
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
    /// How far off the trigger the panel sits, in pixels.
    ///
    /// A tooltip that draws an arrow is held at least [`ARROW_REACH`](crate::tooltip::ARROW_REACH)
    /// away, because the arrow stands outside the panel and would otherwise lie on the control.
    #[prop(default = crate::tooltip::DEFAULT_OFFSET)]
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
    let offset = side_offset(offset, arrow);
    let intent = HoverIntent::current();
    let state = intent.as_ref().map_or_else(
        || OverlayState::uncontrolled(false, None),
        HoverIntent::state,
    );

    let on_enter = intent.clone();
    let on_leave = intent.clone();
    let instant = intent.as_ref().map(HoverIntent::instant);
    let own = Attrs::new()
        .class_toggle(ClassName::new(TooltipStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-tooltip"), true)
        // Set on a tooltip taken down to show the next one, which then goes with no exit.
        .attribute(AttrName::new("data-instant"), move || {
            instant
                .is_some_and(|instant| instant.get())
                .then(String::new)
        })
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
