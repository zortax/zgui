//! One answer to "how long before a tooltip appears", for a whole subtree.

use core::time::Duration;

use zgui::prelude::*;
use zgui::{component, view};

/// The two delays every [`Tooltip`](crate::Tooltip) under a [`TooltipProvider`] uses unless it
/// says otherwise.
///
/// `Copy`, and reachable from any depth with [`TooltipDelays::current`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TooltipDelays {
    /// How long the pointer has to stay on a trigger before its tooltip appears.
    open: Duration,
    /// How long a tooltip stays up after the pointer leaves.
    close: Duration,
}

impl TooltipDelays {
    /// The pair, as a provider hands them down.
    #[must_use]
    pub const fn new(open: Duration, close: Duration) -> Self {
        Self { open, close }
    }

    /// Publishes these to every scope below the current one, and hands them back.
    pub fn provide(self) -> Self {
        provide_local_context(self);
        self
    }

    /// The delays the calling scope is under, when it is under any.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// How long the pointer has to stay before a tooltip appears.
    #[must_use]
    pub const fn open(self) -> Duration {
        self.open
    }

    /// How long a tooltip stays after the pointer leaves.
    #[must_use]
    pub const fn close(self) -> Duration {
        self.close
    }
}

/// Sets the delay for every [`Tooltip`](crate::Tooltip) inside it.
///
/// A tooltip's delay is a property of the *interface*, not of the tooltip: a toolbar whose buttons
/// each answered after a different pause would feel broken rather than considered. So it is set
/// once, around whatever region shares a feel, and a tooltip that genuinely differs still overrides
/// it with its own `delay`.
///
/// It renders no element of its own.
///
/// ```
/// use core::time::Duration;
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui::tooltip::{TooltipProvider, TooltipProviderProps};
///
/// /// A toolbar whose tooltips all wait the same quarter second.
/// #[component]
/// fn Toolbar() -> impl IntoView {
///     view! {
///         TooltipProvider(delay = Duration::from_millis(250)) {
///             Tooltip {
///                 TooltipTrigger {Button(size = ButtonSize::Icon) {"B"}}
///                 TooltipContent {"Bold"}
///             }
///             Tooltip {
///                 TooltipTrigger {Button(size = ButtonSize::Icon) {"I"}}
///                 TooltipContent {"Italic"}
///             }
///         }
///     }
/// }
/// ```
#[component]
pub fn TooltipProvider(
    /// How long the pointer has to stay on a trigger before its tooltip appears.
    #[prop(default = crate::tooltip::DEFAULT_DELAY)]
    delay: Duration,
    /// How long a tooltip stays after the pointer leaves.
    #[prop(default = crate::tooltip::DEFAULT_CLOSE_DELAY)]
    close_delay: Duration,
    /// The tooltips.
    children: Children,
) -> impl IntoView {
    TooltipDelays::new(delay, close_delay).provide();
    view! { {children.into_view_once()} }
}
