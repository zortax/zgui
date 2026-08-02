//! What a tooltip is about.

use zgui::prelude::*;
use zgui::view::AttrName;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};

use crate::overlay::{HoverIntent, OverlayState};
use crate::tooltip::SHEET;
use crate::tooltip::style::TooltipStyle;

/// Wraps whatever a [`Tooltip`](crate::Tooltip) describes.
///
/// It is a wrapper rather than a control of its own, because what it wraps already is one: an
/// icon-only button whose picture needs a name is still the button, and turning it into two nested
/// controls would give a reader two things to meet where there is one.
///
/// The described element is the tooltip's surface, so a reader is told what it says without the
/// tooltip ever having to be shown — which is what a tooltip is *for* to anyone who cannot see it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Tooltip {
///         TooltipTrigger {Button(size = ButtonSize::Icon) {"B"}}
///         TooltipContent {"Bold"}
///     }
/// }
/// # }
/// ```
#[component]
pub fn TooltipTrigger(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the tooltip is about.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, TooltipStyle::CSS);
    let intent = HoverIntent::current();
    let state = intent.as_ref().map_or_else(
        || OverlayState::uncontrolled(false, None),
        HoverIntent::state,
    );

    let own = Attrs::new()
        .attribute(AttrName::new("data-state"), move || {
            Some(state.state_name().to_owned())
        })
        .a11y_from(A11yBinding::unspecified().described_by(state.content()));

    let on_enter = intent.clone();
    let on_leave = intent.clone();
    // Focus is not a pointer that might not have meant it: a keyboard user reached this control on
    // purpose, so the tooltip is shown at once rather than after a delay meant for a pointer
    // travelling past.
    let on_focus = intent.clone();
    let on_blur = intent.clone();
    let on_escape = intent.clone();
    let on_key_down = handler(
        events::KEY_DOWN,
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            if ev.key == Key::Named(NamedKey::Escape)
                && let Some(intent) = &on_escape
                && intent.state().is_open_untracked()
            {
                // Only when something was showing: a tooltip that swallowed every Escape would
                // stop the dialog around it from ever closing.
                ev.stop_propagation();
                intent.close_now();
            }
        },
    );

    view! {
        box(
            class = "zui-tooltip__trigger",
            node_ref = {state.trigger()},
            on:pointer_enter = move |_| { if let Some(intent) = &on_enter { intent.enter() } },
            on:pointer_leave = move |_| { if let Some(intent) = &on_leave { intent.leave() } },
            on:focus_in = move |_| {
                if let Some(intent) = &on_focus {
                    intent.close_now();
                    intent.state().open();
                }
            },
            on:focus_out = move |_| { if let Some(intent) = &on_blur { intent.close_now() } },
            on:key_down = on_key_down,
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
