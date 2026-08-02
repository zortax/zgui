//! What a hover card previews.

use zgui::prelude::*;
use zgui::vocab::{HasPopup, Key, NamedKey};
use zgui::{component, view};

use crate::hover_card::SHEET;
use crate::hover_card::style::HoverCardStyle;
use crate::overlay::{HoverIntent, OverlayState};

/// Wraps whatever a [`HoverCard`](crate::HoverCard) previews.
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
///         HoverCardContent {text {"Ada Lovelace"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn HoverCardTrigger(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the card previews.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, HoverCardStyle::CSS);
    let intent = HoverIntent::current();
    let state = intent.as_ref().map_or_else(
        || OverlayState::uncontrolled(false, None),
        HoverIntent::state,
    );

    let own = state.trigger_attrs(HasPopup::Dialog);

    let on_enter = intent.clone();
    let on_leave = intent.clone();
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
                ev.stop_propagation();
                intent.close_now();
            }
        },
    );

    view! {
        box(
            class = "zui-hover-card__trigger",
            node_ref = {state.trigger()},
            tabindex = {Focus::Sequential},
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
