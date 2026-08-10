//! The headless half of a component library: the interaction behaviours every visible component
//! is built out of, with no appearance of their own.
//!
//! A component library's hard parts are not its looks. They are collision-aware positioning, focus
//! trapping, exit animations that actually finish, dismissal that knows which surface a press
//! belongs to, item order in a tree that reorders itself, and state that works whether the caller
//! owns it or the component does. Written once, every visible component is thin. Written per
//! component, forty components get them forty subtly different ways.
//!
//! Nothing here renders anything you can see. That is the point: appearance is CSS and belongs in
//! a style sheet, so these carry `data-` attributes for a sheet to select on and decide nothing
//! about how anything looks.
//!
//! | Behaviour | The question it answers |
//! |---|---|
//! | [`Popper`] | where does this floating surface go, and does it still fit? |
//! | [`FocusScope`] | may focus leave, and where does it go back to? |
//! | [`RovingFocus`] | what does the next arrow key do? |
//! | [`Presence`] | has the exit animation finished yet? |
//! | [`DismissableLayer`] | does this press belong to me, or to something above me? |
//! | [`Collection`] | what are my items, in the order a reader meets them? |
//! | [`Controllable`] | who owns this value — me, or my caller? |
//! | [`Binding`] | what did the caller tie it to, and what does a click therefore do? |
//!
//! # An ordinary downstream consumer
//!
//! This crate depends on `zgui` and on nothing else. Every behaviour here is written against
//! exactly the public API an application author has — `NodeRef` and its observations, the typed
//! events, the focus traversal, the overlay bands, the timer heap — so anything expressible here
//! is expressible in an application, and anything that were not would be a hole in that API rather
//! than a reason to reach past it.
//!
//! # Putting them together
//!
//! They compose, and a real popover is most of them at once: a [`Presence`] keeps the surface
//! mounted through its exit animation, a [`DismissableLayer`] inside it closes on Escape or on a
//! press past it, and a [`Popper`] inside that places it against the trigger and keeps it on
//! screen.
//!
//! ```no_run
//! use zgui::prelude::*;
//! use zgui::reactive::{RwSignal, UnsyncCallback};
//! use zgui::{component, view};
//! use zgui_ui_primitives::prelude::*;
//!
//! #[component]
//! fn Popover() -> impl IntoView {
//!     let open = RwSignal::new_local(false);
//!     let trigger = NodeRef::new();
//!     let surface = NodeRef::new();
//!
//!     view! {
//!         box {
//!             control(node_ref = trigger, on:click = move |_| open.set(!open.get_untracked())) {
//!                 "Open"
//!             }
//!             Portal {
//!                 Presence(present = Signal::from(open), surface = surface) {
//!                     DismissableLayer(
//!                         on_dismiss = UnsyncCallback::new(move |_: DismissReason| open.set(false))
//!                     ) {
//!                         Popper(anchor = trigger) {
//!                             box(class = "popover", node_ref = surface) {"contents"}
//!                         }
//!                     }
//!                 }
//!             }
//!         }
//!     }
//! }
//! ```
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`popper`] | [`Popper`], [`Placement`], and the pure [`solve`] it is built on |
//! | [`focus`] | [`FocusScope`], [`RovingFocus`] and [`use_roving_item`] |
//! | [`presence`] | [`Presence`], [`PresenceState`], [`use_presence`] and [`Listening`] |
//! | [`dismiss`] | [`DismissableLayer`] and the [`LayerStack`] that orders them |
//! | [`collection`] | [`Collection`], [`CollectionItem`] and [`ItemId`] |
//! | [`state`] | [`Binding`] and [`Controllable`] |
//! | [`prelude`] | all of the above, in one import |

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod collection;
mod diag;
pub mod dismiss;
pub mod focus;
pub mod popper;
pub mod prelude;
pub mod presence;
pub mod state;

pub use crate::collection::{Collection, CollectionItem, ItemId};
pub use crate::dismiss::{
    DismissReason, DismissableLayer, DismissableLayerProps, LayerId, LayerStack,
};
pub use crate::focus::{
    FocusScope, FocusScopeProps, Orientation, RovingContext, RovingFocus, RovingFocusProps,
    RovingItem, use_roving_item, use_roving_item_when,
};
pub use crate::popper::{
    Align, Placement, Popper, PopperOptions, PopperProps, Side, Solution, solve,
};
pub use crate::presence::{
    Listening, Presence, PresenceContext, PresenceProps, PresenceState, use_presence,
};
pub use crate::state::{Binding, Controllable};
