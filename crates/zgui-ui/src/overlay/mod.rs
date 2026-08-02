//! What every surface that floats above the window is built out of.
//!
//! Twelve components in this library render something that is not where it was written: a dialog,
//! an alert dialog, a sheet, a drawer, a popover, a tooltip, a hover card, two menus, a select, a
//! combobox and a command palette. Every one of them needs the same five things, and every one of
//! them gets them wrong differently if it writes them itself:
//!
//! | | |
//! |---|---|
//! | portalled | so the surface escapes whatever clipped or transformed ancestor its trigger lives in |
//! | kept present | so an exit animation finishes before anything is unmounted |
//! | dismissable | so exactly one open surface answers a press past it or an Escape |
//! | placed | so it goes on the side there is room on, and stays inside the window |
//! | confined | so a modal surface cannot be tabbed out of, and gives focus back afterwards |
//!
//! So they are written once. [`AnchoredSurface`] is the four that float beside a trigger;
//! [`ModalSurface`] is the four that take the window over. Both are ordinary compositions of the
//! headless behaviours in [`zgui_ui_primitives`], and both are usable directly by an application
//! building a floating surface this library does not ship.
//!
//! # What is above what
//!
//! Portalling a surface takes it out of its trigger's clipped or transformed ancestors; it does not
//! on its own say which surface is drawn over which. That is the band it lands on, and the band a
//! component asks for describes *what kind of surface it is* rather than *what it was opened from*
//! — so a select, which is a popover, asked for the popover band even when it was opened from
//! inside a dialog, and its list was drawn under the panel that opened it.
//!
//! So the band a surface asks for is a floor. Every surface here publishes where it ended up to
//! everything built inside it, and every surface takes the higher of the band it asked for and the
//! band it was opened from, one step deeper. A select inside a dialog rises to the dialog's band
//! and one above it; a popover inside that select rises again; a toast raised from any of them
//! keeps its own band, above them all. The depth is published as `--zui-overlay-depth` and read by
//! the shared sheet as the `z-index` of the boxes a band stacks, because mount order cannot be
//! relied on for it: a portal written inside another portal's content reaches the band *first*, so
//! left to itself the inner surface would be painted underneath. [`SurfaceElevation`] is that rule,
//! and it is the only thing that decides it.
//!
//! Dismissing is a separate question with the same answer: one dismissable band for everything, so
//! the innermost surface open is the one that answers a press or an Escape.
//!
//! # The three parts of an overlay
//!
//! A root that owns whether it is open, a trigger inside it, and a surface portalled out of it.
//! They are three components apart and each needs the same facts, so the root publishes an
//! [`OverlayState`] and the other two find it. Nothing is passed down by hand.
//!
//! ```no_run
//! use zgui::prelude::*;
//! use zgui::{component, view};
//! use zgui_ui::overlay::{AnchoredSurface, AnchoredSurfaceProps, OverlayState};
//!
//! /// A surface of one's own, on the same machinery every component here uses.
//! #[component]
//! fn Hint() -> impl IntoView {
//!     let state = OverlayState::uncontrolled(false, None).provide();
//!     view! {
//!         box {
//!             control(
//!                 node_ref = {state.trigger()},
//!                 {..state.trigger_attrs(zgui::vocab::HasPopup::Dialog)},
//!                 on:click = move |_| state.toggle()
//!             ) {
//!                 "What is this?"
//!             }
//!             AnchoredSurface(state = state, role = Role::Dialog) {
//!                 text {"A hint."}
//!             }
//!         }
//!     }
//! }
//! ```

mod anchored;
mod content;
mod delay;
mod elevation;
mod hover;
mod labels;
mod lock;
mod modal;
mod state;
mod style;

pub use crate::overlay::anchored::{
    AnchoredSurface, AnchoredSurfaceProps, Confined, ConfinedProps,
};
pub use crate::overlay::content::{OverlaySurface, OverlaySurfaceProps};
pub use crate::overlay::delay::Delayed;
pub use crate::overlay::elevation::{Elevated, ElevatedProps, SurfaceElevation};
pub use crate::overlay::hover::HoverIntent;
pub use crate::overlay::labels::SurfaceLabels;
pub use crate::overlay::lock::{ScrollLock, ScrollLockGuard, use_scroll_lock};
pub use crate::overlay::modal::{ModalSurface, ModalSurfaceProps, Scrim, ScrimProps};
pub use crate::overlay::state::OverlayState;
pub use crate::overlay::style::OverlayStyle;

/// What the shared overlay rules are installed under.
pub(crate) const SHEET: &str = "zui-overlay";
