//! A preview that appears beside a link when the pointer rests on it.

mod content;
mod style;
mod trigger;

pub use crate::hover_card::content::{HoverCardContent, HoverCardContentProps};
pub use crate::hover_card::style::HoverCardStyle;
pub use crate::hover_card::trigger::{HoverCardTrigger, HoverCardTriggerProps};

use core::time::Duration;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::overlay::{HoverIntent, OverlayState};
use zgui_ui_primitives::Binding;

/// What the hover card's rules are installed under.
pub(crate) const SHEET: &str = "zui-hover-card";

/// How long the pointer rests on a trigger before its card appears.
pub const DEFAULT_DELAY: Duration = Duration::from_millis(700);

/// How long a card stays up after the pointer leaves it and its trigger.
pub const DEFAULT_CLOSE_DELAY: Duration = Duration::from_millis(300);

/// A preview of what is behind a link, shown when the pointer rests on it.
///
/// Where a [`Tooltip`](crate::Tooltip) names a control in three words, a hover card previews
/// something: an avatar, a summary, a couple of figures. That difference decides everything else
/// about it — it is announced as a surface rather than as a description, its closing delay is long
/// enough for the pointer to travel onto it, and it stays open while the pointer is on it.
///
/// It is still a *preview*: nothing in it may be the only way to do something. A pointer is the
/// only thing that opens it, so anything reachable only from inside it is reachable only with a
/// pointer.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Who wrote this, without leaving the page.
/// #[component]
/// fn Byline() -> impl IntoView {
///     view! {
///         HoverCard {
///             HoverCardTrigger {text {"@ada"}}
///             HoverCardContent {
///                 row {Avatar(label = "Ada") {"AL"}text {"Ada Lovelace"}}
///                 text {"Joined December 1842"}
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// Focusing the trigger opens it and leaving closes it, both without a delay. <kbd>Escape</kbd>
/// closes it.
#[component]
pub fn HoverCard(
    /// Whether it is showing, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    open: Binding<bool>,
    /// Whether it starts showing, when it owns that itself.
    #[prop(default = false)]
    default_open: bool,
    /// Told whenever it appears or goes, whoever owns it.
    #[prop(optional)]
    on_open_change: Option<UnsyncCallback<bool>>,
    /// How long the pointer has to rest before it appears.
    #[prop(default = DEFAULT_DELAY)]
    delay: Duration,
    /// How long it stays after the pointer leaves.
    #[prop(default = DEFAULT_CLOSE_DELAY)]
    close_delay: Duration,
    /// The trigger and the content.
    children: Children,
) -> impl IntoView {
    let state = OverlayState::new(open, default_open, on_open_change).provide();
    HoverIntent::new(state, delay, close_delay).provide();
    view! { {children.into_view_once()} }
}
