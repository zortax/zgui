//! A short label that appears beside what it describes.

mod arrow;
mod content;
mod provider;
mod style;
mod trigger;

pub use crate::tooltip::arrow::{TooltipArrow, TooltipArrowProps};
pub use crate::tooltip::content::{TooltipContent, TooltipContentProps};
pub use crate::tooltip::provider::{TooltipDelays, TooltipProvider, TooltipProviderProps};
pub use crate::tooltip::style::TooltipStyle;
pub use crate::tooltip::trigger::{TooltipTrigger, TooltipTriggerProps};

use core::time::Duration;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::overlay::{HoverIntent, OverlayState};
use zgui_ui_primitives::Binding;

/// What the tooltip's rules are installed under.
pub(crate) const SHEET: &str = "zui-tooltip";

/// How long the pointer has to stay on a trigger before its tooltip appears.
///
/// None at all. A tooltip names a control whose picture is not enough, and a name that arrives
/// three quarters of a second after the pointer does has already lost the moment it was wanted in.
/// A whole region can be given a pause instead with
/// [`TooltipProvider`].
pub const DEFAULT_DELAY: Duration = Duration::ZERO;

/// How long a tooltip stays up after the pointer leaves.
pub const DEFAULT_CLOSE_DELAY: Duration = Duration::from_millis(150);

/// A label that appears beside a control after the pointer has stayed on it.
///
/// A tooltip names something whose picture is not enough — an icon-only button, a truncated
/// column. It is not a place to put anything to operate: it takes no focus, it holds no controls,
/// and it goes away as soon as the pointer does. A surface with something to click in it is a
/// [`HoverCard`](crate::HoverCard) or a [`Popover`](crate::Popover).
///
/// ```
/// use core::time::Duration;
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// An icon-only button, named for anyone who cannot tell what the icon means.
/// #[component]
/// fn Toolbar() -> impl IntoView {
///     view! {
///         Tooltip(delay = Duration::from_millis(400)) {
///             TooltipTrigger {Button(size = ButtonSize::Icon) {"B"}}
///             TooltipContent {"Bold"}
///         }
///     }
/// }
/// ```
///
/// # The two delays
///
/// Opening is delayed so that dragging the pointer across a toolbar does not raise and drop a
/// tooltip per button. Closing is delayed by much less, so that moving between two neighbouring
/// buttons does not flicker. Both run on the engine's own timer heap, so a test that advances the
/// clock by hand fires them exactly as a running window does.
///
/// # Keyboard
///
/// Focusing the trigger shows it and leaving hides it, with no delay either way: a keyboard user
/// asked for this control deliberately. <kbd>Escape</kbd> hides it without moving focus.
#[component]
pub fn Tooltip(
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
    /// How long the pointer has to stay before it appears.
    ///
    /// Whatever the enclosing [`TooltipProvider`] says, when this is left out and there is one.
    #[prop(into, optional)]
    delay: Option<Duration>,
    /// How long it stays after the pointer leaves.
    ///
    /// Whatever the enclosing [`TooltipProvider`] says, when this is left out and there is one.
    #[prop(into, optional)]
    close_delay: Option<Duration>,
    /// The trigger and the content.
    children: Children,
) -> impl IntoView {
    let shared = TooltipDelays::current();
    let delay = delay
        .or_else(|| shared.map(TooltipDelays::open))
        .unwrap_or(DEFAULT_DELAY);
    let close_delay = close_delay
        .or_else(|| shared.map(TooltipDelays::close))
        .unwrap_or(DEFAULT_CLOSE_DELAY);
    let state = OverlayState::new(open, default_open, on_open_change).provide();
    HoverIntent::new(state, delay, close_delay).provide();
    view! { {children.into_view_once()} }
}
