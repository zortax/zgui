//! The panel a hover card previews into.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::ClassName;
use zgui::{component, view};
use zgui_ui_primitives::Placement;

use crate::hover_card::SHEET;
use crate::hover_card::style::HoverCardStyle;
use crate::overlay::{AnchoredSurfaceProps, HoverIntent, OverlayState};

/// The preview itself, which stays up while the pointer is on it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     HoverCard {
///         HoverCardTrigger {text {"@ada"}}
///         HoverCardContent {text {"Joined December 1842"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn HoverCardContent(
    /// Where it is asked to go, before the window's edges have their say.
    #[prop(into, default = Signal::stored_local(Placement::BOTTOM))]
    placement: Signal<Placement, LocalStorage>,
    /// How far off the trigger it sits, in pixels.
    #[prop(default = 4.0)]
    offset: f32,
    /// Classes merged after the card's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the panel.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the preview shows.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, HoverCardStyle::CSS);
    let intent = HoverIntent::current();
    let state = intent.as_ref().map_or_else(
        || OverlayState::uncontrolled(false, None),
        HoverIntent::state,
    );

    let on_enter = intent.clone();
    let on_leave = intent.clone();
    let own = Attrs::new()
        .class_toggle(ClassName::new(HoverCardStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-hover-card"), true)
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
            role = {Role::Dialog},
            dismiss_on_outside_press = {false},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.view()}
        }
    }
}
